use crate::adapter::{self, NetworkAdapter, Endpoint, AdapterEvent, ResourceId, ResourceType,
    SharedResourceIdGenerator};

use mio::net::{TcpListener, TcpStream};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr};
use std::time::{Duration};
use std::collections::{HashMap};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering, AtomicUsize},
};
use std::thread::{self, JoinHandle};
use std::io::{ErrorKind, Read};
use std::ops::{Deref};

const INPUT_BUFFER_SIZE: usize = 65536;
const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const EVENTS_SIZE: usize = 1024;

const OTHER_THREAD_ERR: &'static str = "This error is shown because other thread has panicked";

struct Store {
    streams: RwLock<HashMap<ResourceId, (Arc<TcpStream>, SocketAddr)>>,
    listeners: Mutex<HashMap<ResourceId, TcpListener>>,
    id_generator: SharedResourceIdGenerator,
    registry: Registry,
}

impl Store {
    fn new(registry: Registry) -> Store {
        Store {
            streams: RwLock::new(HashMap::new()),
            listeners: Mutex::new(HashMap::new()),
            id_generator: SharedResourceIdGenerator::new(),
            registry,
        }
    }
}

pub struct TcpAdapter {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    store: Arc<Store>,
}

impl NetworkAdapter for TcpAdapter {
    fn init<C>(mut event_callback: C) -> TcpAdapter where
    C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) + Send + 'static {

        let poll = Poll::new().unwrap();
        let store = Store::new(poll.registry().try_clone().unwrap()); //TODO: clone?
        let store = Arc::new(store);
        let thread_store = store.clone();


        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let thread = thread::Builder::new()
            .name("message-io: tcp-adapter".into())
            .spawn(move || {
                let mut input_buffer = [0; INPUT_BUFFER_SIZE];
                let timeout = Some(Duration::from_millis(NETWORK_SAMPLING_TIMEOUT));
                let mut event_processor =
                    TcpEventProcessor::new(thread_store, &mut input_buffer[..], timeout, poll);

                while running.load(Ordering::Relaxed) {
                    event_processor.process(&mut event_callback);
                }
            })
            .unwrap();

        TcpAdapter {
            thread: Some(thread),
            thread_running,
            store,
        }
    }

    fn add_remote<R>(&mut self, remote: R) -> Endpoint {
        todo!()
    }

    fn add_listener<L>(&mut self, listener: L) -> (ResourceId, SocketAddr) {
        todo!()
    }

    fn remove_remote(&mut self, id: ResourceId) -> Option<()> {
        todo!()
    }

    fn remove_listener(&mut self, id: ResourceId) -> Option<()> {
        todo!()
    }

    fn local_address(&self, id: ResourceId) -> Option<SocketAddr> {
        todo!()
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) {
        todo!()
    }
}

impl Drop for TcpAdapter {
    fn drop(&mut self) {
        self.thread_running.store(false, Ordering::Relaxed);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect(OTHER_THREAD_ERR);
    }
}

struct TcpEventProcessor<'a> {
    resource_processor: TcpResourceProcessor<'a>,
    timeout: Option<Duration>,
    poll: Poll,
    events: Events,
}

impl<'a> TcpEventProcessor<'a> {
    fn new(
        store: Arc<Store>,
        input_buffer: &'a mut [u8],
        timeout: Option<Duration>,
        poll: Poll,
    ) -> TcpEventProcessor<'a> {
        TcpEventProcessor {
            resource_processor: TcpResourceProcessor::new(store, input_buffer),
            timeout,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
        }
    }

    pub fn process<C>(
        &mut self,
        event_callback: &mut C,
    ) where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        loop {
            match self.poll.poll(&mut self.events, self.timeout) {
                Ok(_) => break self.process_resource(event_callback),
                Err(e) => match e.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => Err(e).unwrap(),
                },
            }
        }
    }

    fn process_resource<C>(&mut self, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        for mio_event in &self.events {
            let token = mio_event.token();
            let id = ResourceId::from(token.0);
            log::trace!("Wake from poll for TCP resource id {:?}. ", id);

            match id.resource_type() {
                ResourceType::Listener =>
                    self.resource_processor.process_listener(id, event_callback),
                ResourceType::Remote =>
                    self.resource_processor.process_stream(id, event_callback),
            }
        }
    }
}

struct TcpResourceProcessor<'a> {
    store: Arc<Store>,
    input_buffer: &'a mut [u8],
}

impl<'a> TcpResourceProcessor<'a> {
    fn new(store: Arc<Store>, input_buffer: &'a mut [u8]) -> TcpResourceProcessor {
        TcpResourceProcessor { store, input_buffer, }
    }

    fn process_listener<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        // We check the existance of the listener because some event could be produced
        // before removing it.
        if let Some(listener) = self.store.listeners.lock().expect(OTHER_THREAD_ERR).get(&id) {
            let mut streams = self.store.streams.write().expect(OTHER_THREAD_ERR);
            loop {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        let id = self.store.id_generator.generate(ResourceType::Remote); //TODO
                        streams.insert(id, (Arc::new(stream), addr));

                        let endpoint = Endpoint::new(id, addr);
                        event_callback(endpoint, AdapterEvent::Connection);
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(err) => Err(err).unwrap(),
                }
            }
        }
    }

    fn process_stream<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        let remove =
        if let Some((stream, addr)) = self.store.streams.read().expect(OTHER_THREAD_ERR).get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            loop {
                match stream.deref().read(&mut self.input_buffer) {
                    Ok(0) => {
                        event_callback(endpoint, AdapterEvent::Disconnection);
                        break true
                    }
                    Ok(size) => {
                        event_callback(endpoint, AdapterEvent::Data(&self.input_buffer[..size]));
                    }
                    Err(ref err) if err.kind() == ErrorKind::ConnectionReset => {
                        event_callback(endpoint, AdapterEvent::Disconnection);
                        break true
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break false,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(err) => Err(err).unwrap(),
                }
            }

        }
        else { false };

        if !remove {
            self.store.streams.write().expect(OTHER_THREAD_ERR).remove(&id).unwrap();
        }
    }
}
