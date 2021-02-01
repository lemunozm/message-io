use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};
use crate::util::{OTHER_THREAD_ERR};

use mio::net::{TcpListener, TcpStream};
use mio::{Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::time::{Duration};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};

const INPUT_BUFFER_SIZE: usize = 65536;
const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const EVENTS_SIZE: usize = 1024;

#[derive(Debug)]
pub enum TcpEvent<'a> {
    Connection,
    Data(&'a [u8]),
    Disconnection,
}

pub struct TcpAdapter {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    store: Arc<Store>,
}

impl TcpAdapter {
    pub fn init<C>(adapter_id: u8, mut event_callback: C) -> Self
    where C: for<'b> FnMut(Endpoint, TcpEvent<'b>) + Send + 'static {
        let id_generator = ResourceIdGenerator::new(adapter_id);
        let poll = Poll::new().unwrap();
        let store = Store::new(id_generator, poll.registry().try_clone().unwrap());
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

        Self {
            thread: Some(thread),
            thread_running,
            store,
        }
    }

    pub fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint> {
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        let mut stream = TcpStream::from_std(stream);

        let id = self.store.id_generator.generate(ResourceType::Remote);
        self.store.registry.register(&mut stream, Token(id.raw()), Interest::READABLE).unwrap();
        self.store.streams.write().expect(OTHER_THREAD_ERR).insert(id, (Arc::new(stream), addr));
        Ok(Endpoint::new(id, addr))
    }

    pub fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let mut listener = TcpListener::bind(addr)?;

        let id = self.store.id_generator.generate(ResourceType::Listener);
        let real_addr = listener.local_addr().unwrap();
        self.store.registry.register(&mut listener, Token(id.raw()), Interest::READABLE).unwrap();
        self.store.listeners.write().expect(OTHER_THREAD_ERR).insert(id, listener);
        Ok((id, real_addr))
    }

    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        match id.resource_type() {
            ResourceType::Listener =>
                self.store.listeners.write().expect(OTHER_THREAD_ERR).remove(&id)
                    .map(|mut listener|{
                        self.store.registry.deregister(&mut listener).unwrap();
                    }),
            ResourceType::Remote =>
                self.store.streams.write().expect(OTHER_THREAD_ERR).remove(&id)
                    .map(|(stream, _)|{
                        let source = &mut Arc::try_unwrap(stream).unwrap();
                        self.store.registry.deregister(source).unwrap();
                    }),
        }
    }

    pub fn local_address(&self, id: ResourceId) -> Option<SocketAddr> {
        match id.resource_type() {
            ResourceType::Listener =>
                self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id)
                    .map(|listener| listener.local_addr().unwrap()),
            ResourceType::Remote =>
                self.store.streams.read().expect(OTHER_THREAD_ERR).get(&id)
                    .map(|(stream, _)| stream.local_addr().unwrap()),
        }
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) {
        let streams = self.store.streams.read().expect(OTHER_THREAD_ERR);
        let stream = match streams.get(&endpoint.resource_id()) {
            Some((stream, _)) => stream,
            None => panic!("Resource id '{}' doesn't exists in the tcp adapter \
                or is not a remote resource", endpoint.resource_id()),
        };

        // TODO: The current implementation implies an active waiting,
        // improve it using POLLIN instead to avoid active waiting.
        // Note: Despite the fear that an active waiting could generate,
        // this waiting only occurs in the rare case when the send method needs block.
        let mut total_bytes_sent = 0;
        loop {
            match stream.deref().write(&data[total_bytes_sent..]) {
                Ok(bytes_sent) => {
                    total_bytes_sent += bytes_sent;
                    if total_bytes_sent == data.len() {
                        break
                    }
                    // We get sending to data, but not the totality.
                    // We start waiting actively.
                }
                // If WouldBlock is received in this non-blocking socket means that
                // the sending buffer is full and it should wait to send more data.
                // This occurs when huge amounts of data are sent and It could be
                // intensified if the remote endpoint reads slower than this enpoint sends.
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                // Skipping. Others errors will be considered fatal for the connection.
                // We skip here their handling because if the connection brokes,
                // an Event::Disconnection will be generated later.
                Err(_) => break,
            }
        }
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

struct Store {
    // We store the addr because we will need it when the stream crash.
    // When a stream crash by an error (i.e. reset) peer_addr no longer returns the addr.
    streams: RwLock<HashMap<ResourceId, (Arc<TcpStream>, SocketAddr)>>,
    listeners: RwLock<HashMap<ResourceId, TcpListener>>,
    id_generator: ResourceIdGenerator,
    registry: Registry,
}

impl Store {
    fn new(id_generator: ResourceIdGenerator, registry: Registry) -> Store {
        Store {
            streams: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            id_generator,
            registry,
        }
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
        Self {
            resource_processor: TcpResourceProcessor::new(store, input_buffer),
            timeout,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
        }
    }

    pub fn process<C>(
        &mut self,
        event_callback: &mut C,
    ) where C: for<'b> FnMut(Endpoint, TcpEvent<'b>) {
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
    where C: for<'b> FnMut(Endpoint, TcpEvent<'b>) {
        for mio_event in &self.events {
            let id = ResourceId::from(mio_event.token().0);

            log::trace!("Wake from poll for TCP with resource id {}. ", id);

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
    fn new(store: Arc<Store>, input_buffer: &'a mut [u8]) -> Self {
        Self { store, input_buffer, }
    }

    fn process_listener<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, TcpEvent<'b>) {
        // We check the existance of the listener because some event could be produced
        // before removing it.
        if let Some(listener) = self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id) {
            let mut streams = self.store.streams.write().expect(OTHER_THREAD_ERR);
            loop {
                match listener.accept() {
                    Ok((mut stream, addr)) => {
                        let id = self.store.id_generator.generate(ResourceType::Remote);
                        self.store.registry
                            .register(&mut stream, Token(id.raw()), Interest::READABLE)
                            .unwrap();

                        streams.insert(id, (Arc::new(stream), addr));

                        let endpoint = Endpoint::new(id, addr);
                        event_callback(endpoint, TcpEvent::Connection);
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(err) => Err(err).unwrap(),
                }
            }
        }
    }

    fn process_stream<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, TcpEvent<'b>) {
        let must_be_removed =
        if let Some((stream, addr)) = self.store.streams.read().expect(OTHER_THREAD_ERR).get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            loop {
                match stream.deref().read(&mut self.input_buffer) {
                    Ok(0) => {
                        event_callback(endpoint, TcpEvent::Disconnection);
                        break true
                    }
                    Ok(size) => {
                        event_callback(endpoint, TcpEvent::Data(&self.input_buffer[..size]));
                    }
                    Err(ref err) if err.kind() == ErrorKind::ConnectionReset => {
                        event_callback(endpoint, TcpEvent::Disconnection);
                        break true
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break false,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(err) => Err(err).unwrap(),
                }
            }

        }
        else { false };

        // Here the read lock has been dropped and it's safe to perform the write lock
        if must_be_removed {
            self.store.streams.write().expect(OTHER_THREAD_ERR).remove(&id)
                .map(|(stream, _)|{
                    let source = &mut Arc::try_unwrap(stream).unwrap();
                    self.store.registry.deregister(source).unwrap();
                })
                .unwrap();
        }
    }
}
