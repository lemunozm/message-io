use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};
use crate::util::{OTHER_THREAD_ERR};

use mio::{Poll, Interest, Token, Events, Registry};
use mio::net::{UdpSocket};

use net2::{UdpBuilder};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::time::{Duration};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::io::{self, ErrorKind};

pub const MAX_UDP_LEN: usize = 1488;
const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const EVENTS_SIZE: usize = 1024;

pub struct UdpAdapter {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    store: Arc<Store>,
}

impl UdpAdapter {
    pub fn init<C>(adapter_id: u8, mut event_callback: C) -> Self
    where C: for<'b> FnMut(Endpoint, &'b [u8]) + Send + 'static {
        let id_generator = ResourceIdGenerator::new(adapter_id);
        let poll = Poll::new().unwrap();
        let store = Store::new(id_generator, poll.registry().try_clone().unwrap());
        let store = Arc::new(store);
        let thread_store = store.clone();

        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let thread = thread::Builder::new()
            .name("message-io: udp-adapter".into())
            .spawn(move || {
                let mut input_buffer = [0; MAX_UDP_LEN];
                let timeout = Some(Duration::from_millis(NETWORK_SAMPLING_TIMEOUT));
                let mut event_processor =
                    UdpEventProcessor::new(thread_store, &mut input_buffer[..], timeout, poll);

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
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        socket.connect(addr)?;

        let id = self.store.id_generator.generate(ResourceType::Remote);
        self.store.registry.register(&mut socket, Token(id.raw()), Interest::READABLE).unwrap();
        self.store.sockets.write().expect(OTHER_THREAD_ERR).insert(id, (Arc::new(socket), addr));
        Ok(Endpoint::new(id, addr))
    }

    pub fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let socket = UdpSocket::bind(addr)?;
        Ok(self.listen_from_socket(socket))
    }

    pub fn listen_multicast(&mut self, addr: SocketAddrV4) -> io::Result<(ResourceId, SocketAddr)> {
        let listening_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port());
        let socket = UdpBuilder::new_v4()?
            .reuse_address(true)?
            .bind(listening_addr)?;

        socket.set_nonblocking(true)?;
        socket.join_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
        let socket = UdpSocket::from_std(socket);

        Ok(self.listen_from_socket(socket))
    }

    fn listen_from_socket(&mut self, mut socket: UdpSocket) -> (ResourceId, SocketAddr) {
        let id = self.store.id_generator.generate(ResourceType::Listener);
        let real_addr = socket.local_addr().unwrap();
        self.store.registry.register(&mut socket, Token(id.raw()), Interest::READABLE).unwrap();
        self.store.listeners.write().expect(OTHER_THREAD_ERR).insert(id, socket);
        (id, real_addr)
    }

    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        match id.resource_type() {
            ResourceType::Listener =>
                self.store.listeners.write().expect(OTHER_THREAD_ERR).remove(&id)
                    .map(|mut listener|{
                        self.store.registry.deregister(&mut listener).unwrap();
                    }),
            ResourceType::Remote =>
                self.store.sockets.write().expect(OTHER_THREAD_ERR).remove(&id)
                    .map(|(socket, _)|{
                        let source = &mut Arc::try_unwrap(socket).unwrap();
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
                self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&id)
                    .map(|(socket, _)| socket.local_addr().unwrap()),
        }
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) {
        assert!(data.len() <= MAX_UDP_LEN,
            "The datagram max size is {} bytes, but your message data takes {} bytes. \
            Split the message in several messages or use a stream protocol as TCP",
            MAX_UDP_LEN, data.len());

        // Only two errors can happend in 'sends' methods of UDP:
        // A packet that exceeds MTU size and send() called without knowing the remote addr.

        let mut message_sent = false;

        {
            let sockets = self.store.sockets.read().expect(OTHER_THREAD_ERR);
            if let Some((socket, _)) = sockets.get(&endpoint.resource_id()) {
                socket.send(data).expect("No errors here");
                message_sent = true;
            }
        }

        {
            let listeners = self.store.listeners.read().expect(OTHER_THREAD_ERR);
            if let Some(socket) = listeners.get(&endpoint.resource_id()) {
                socket.send_to(data, endpoint.addr()).expect("No errors here");
                message_sent = true;
            }
        }

        if !message_sent {
            panic!("Resource id '{}' doesn't exists", endpoint.resource_id());
        }
    }
}

impl Drop for UdpAdapter {
    fn drop(&mut self) {
        for socket in self.store.listeners.write().expect(OTHER_THREAD_ERR).values_mut() {
            if let SocketAddr::V4(addr) = socket.local_addr().unwrap() {
                if addr.ip().is_multicast() {
                    socket.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
                }
            }
        }

        self.thread_running.store(false, Ordering::Relaxed);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect(OTHER_THREAD_ERR);
    }
}

struct Store {
    sockets: RwLock<HashMap<ResourceId, (Arc<UdpSocket>, SocketAddr)>>,
    listeners: RwLock<HashMap<ResourceId, UdpSocket>>,
    id_generator: ResourceIdGenerator,
    registry: Registry,
}

impl Store {
    fn new(id_generator: ResourceIdGenerator, registry: Registry) -> Store {
        Store {
            sockets: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            id_generator,
            registry,
        }
    }
}

struct UdpEventProcessor<'a> {
    resource_processor: UdpResourceProcessor<'a>,
    timeout: Option<Duration>,
    poll: Poll,
    events: Events,
}

impl<'a> UdpEventProcessor<'a> {
    fn new(
        store: Arc<Store>,
        input_buffer: &'a mut [u8],
        timeout: Option<Duration>,
        poll: Poll,
    ) -> UdpEventProcessor<'a> {
        Self {
            resource_processor: UdpResourceProcessor::new(store, input_buffer),
            timeout,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
        }
    }

    pub fn process<C>(
        &mut self,
        event_callback: &mut C,
    ) where C: for<'b> FnMut(Endpoint, &'b [u8]) {
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
    where C: for<'b> FnMut(Endpoint, &'b [u8]) {
        for mio_event in &self.events {
            let id = ResourceId::from(mio_event.token().0);

            log::trace!("Wake from poll for UDP with resource id {}. ", id);

            match id.resource_type() {
                ResourceType::Listener =>
                    self.resource_processor.process_listener_socket(id, event_callback),
                ResourceType::Remote =>
                    self.resource_processor.process_remote_socket(id, event_callback),
            }
        }
    }
}

struct UdpResourceProcessor<'a> {
    store: Arc<Store>,
    input_buffer: &'a mut [u8],
}

impl<'a> UdpResourceProcessor<'a> {
    fn new(store: Arc<Store>, input_buffer: &'a mut [u8]) -> Self {
        Self { store, input_buffer, }
    }

    fn process_listener_socket<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, &'b [u8]) {
        // We check the existance of the listener because some event could be produced
        // before removing it.
        if let Some(socket) = self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id) {
            loop {
                match socket.recv_from(&mut self.input_buffer) {
                    Ok((size, addr)) => event_callback(
                        Endpoint::new(id, addr),
                        &mut self.input_buffer[..size],
                    ),
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => Err(err).unwrap(),
                }
            }
        }
    }

    fn process_remote_socket<C>(&mut self, id: ResourceId, event_callback: &mut C)
    where C: for<'b> FnMut(Endpoint, &'b [u8]) {
        if let Some((socket, addr)) = self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            loop {
                match socket.recv(&mut self.input_buffer) {
                    Ok(size) => event_callback(endpoint, &mut self.input_buffer[..size]),
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => continue,
                    Err(err) => Err(err).unwrap(),
                }
            }
        }
    }
}
