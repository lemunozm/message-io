use crate::adapter::{NetworkAdapter, Endpoint, ResourceId, AdapterEvent};

use mio::net::{TcpListener, TcpStream};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr};
use std::time::{Duration};
use std::collections::{HashMap};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::io::{self, prelude::*, ErrorKind};

const INPUT_BUFFER_SIZE: usize = 65536;
const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const EVENTS_SIZE: usize = 1024;

const POISONED_LOCK: &'static str = "This error is shown because other thread has panicked";

enum TcpResource {
    Listener(TcpListener),

    // The SocketAddr is stored in order to retrive it when the stream has failed.
    // When a TcpStream fails its address can not be retrived.
    Remote(TcpStream, SocketAddr),
}

impl std::fmt::Display for TcpResource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let resource = match self {
            TcpResource::Listener(_) => "TcpResource::Listener",
            TcpResource::Remote(..) => "TcpResource::Remote",
        };
        write!(f, "{}", resource)
    }
}

struct Store {
    resources: HashMap<ResourceId, TcpResource>,
    last_id: ResourceId,
    registry: Registry,
}

impl Store {
    fn new(registry: Registry) -> Store {
        Store { resources: HashMap::new(), last_id: 0, registry }
    }

    fn add(&mut self, resource: TcpResource) -> Endpoint {
        todo!()
    }

    fn resource(&self, id: ResourceId) -> Option<TcpResource> {
        todo!()
    }

    fn remove(&mut self, id: ResourceId) -> Option<Endpoint> {
        todo!()
    }
}

pub struct TcpAdapter {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    store: Arc<Mutex<Store>>,
}

impl NetworkAdapter for TcpAdapter {
    fn init<C>(mut event_callback: C) -> TcpAdapter where
    C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) + Send + 'static {

        let poll = Poll::new().unwrap();
        let store = Store::new(poll.registry().try_clone().unwrap());
        let thread_safe_store = Arc::new(Mutex::new(store));

        let mut event_processor = TcpEventProcessor::new(thread_safe_store.clone(), poll);

        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let thread = thread::Builder::new()
            .name("message-io: network".into())
            .spawn(move || {
                let mut input_buffer = [0; INPUT_BUFFER_SIZE];
                let timeout = Duration::from_millis(NETWORK_SAMPLING_TIMEOUT);
                while running.load(Ordering::Relaxed) {
                    event_processor.process(
                        &mut input_buffer[..], // TODO: To constructor
                        Some(timeout), //TODO to constructor
                        &mut event_callback,
                    );
                }
            })
            .unwrap();

        TcpAdapter {
            thread: Some(thread),
            thread_running,
            store: thread_safe_store,
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

pub struct TcpEventProcessor {
    store: Arc<Mutex<Store>>,
    poll: Poll,
    events: Events,
}

impl TcpEventProcessor {
    fn new(store: Arc<Mutex<Store>>, poll: Poll) -> TcpEventProcessor {
        TcpEventProcessor { store, poll, events: Events::with_capacity(EVENTS_SIZE) }
    }

    pub fn process<C>(
        &mut self,
        input_buffer: &mut [u8],
        timeout: Option<Duration>,
        event_callback: &mut C,
    ) where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        loop {
            match self.poll.poll(&mut self.events, timeout) {
                Ok(_) => break self.process_event(input_buffer, event_callback),
                Err(e) => match e.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => Err(e).unwrap(),
                },
            }
        }
    }

    fn process_event<C>(&mut self, input_buffer: &mut [u8], event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) {
        for mio_event in &self.events {
            let token = mio_event.token();
            let id = token.0;
            let mut store = self.store.lock().expect(POISONED_LOCK);

            let resource = store.resources.get_mut(&id).unwrap();
            log::trace!("Wake from poll for endpoint {}. Resource: {}", id, resource);
            match resource {
                TcpResource::Listener(listener) => {
                    let mut listener = listener;
                    loop {
                        match listener.accept() {
                            Ok((stream, addr)) => {
                                let endpoint = store.add(TcpResource::Remote(stream, addr));
                                event_callback(endpoint, AdapterEvent::Connection);

                                // Used to avoid consecutive mutable borrows
                                listener = match store.resources.get_mut(&id).unwrap() {
                                    TcpResource::Listener(listener) => listener,
                                    _ => unreachable!(),
                                }
                            }
                            Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                            Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                            Err(err) => Err(err).unwrap(),
                        }
                    }
                },
                TcpResource::Remote(stream, addr) => loop {
                    let endpoint = Endpoint::new(id, *addr);
                    match stream.read(input_buffer) {
                        Ok(0) => {
                            store.remove(endpoint.resource_id()).unwrap();
                            event_callback(endpoint, AdapterEvent::Disconnection);
                            break
                        }
                        Ok(size) => {
                            event_callback(endpoint, AdapterEvent::Data(&input_buffer[..size]));
                        }
                        Err(ref err) if err.kind() == ErrorKind::ConnectionReset => {
                            store.remove(endpoint.resource_id()).unwrap();
                            event_callback(endpoint, AdapterEvent::Disconnection);
                            break
                        }
                        Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                        Err(err) => Err(err).unwrap(),
                    }
                },
            }
        }
    }
}
