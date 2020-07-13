use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use std::collections::{HashMap};
use std::io::{self, prelude::*};

const EVENTS_SIZE: usize = 1024;
const INPUT_BUFFER_SIZE: usize = 65536;

pub enum Event<'a> {
    Connection(SocketAddr),
    Data(&'a[u8]),
    Disconnection,
}

pub fn adapter() -> (Controller, Receiver) {
    let poll = Poll::new().unwrap();
    let store = Arc::new(Mutex::new(Store::new(poll.registry().try_clone().unwrap())));
    (Controller::new(store.clone()), Receiver::new(store, poll))
}

pub enum Connection {
    TcpListener(TcpListener),
    TcpStream(TcpStream),
    UdpListener(UdpSocket),
    UdpSocket(UdpSocket, SocketAddr),
}

impl Connection {
    pub fn new_tcp_listener(addr: SocketAddr) -> io::Result<Connection> {
        TcpListener::bind(addr).map(|listener| Connection::TcpListener(listener))
    }

    pub fn new_tcp_stream(addr: SocketAddr) -> io::Result<Connection> {
        StdTcpStream::connect(addr).map(|stream| Connection::TcpStream(TcpStream::from_std(stream)))
    }

    pub fn new_udp_listener(addr: SocketAddr) -> io::Result<Connection> {
        UdpSocket::bind(addr).map(|socket| Connection::UdpListener(socket))
    }

    pub fn new_udp_socket(addr: SocketAddr) -> io::Result<Connection> {
        UdpSocket::bind("0.0.0.0:0".parse().unwrap()).map(|socket| {
            socket.connect(addr).unwrap();
            Connection::UdpSocket(socket, addr)
        })
    }

    pub fn event_source(&mut self) -> &mut dyn event::Source {
        match self {
            Connection::TcpListener(listener) => listener,
            Connection::TcpStream(stream) => stream,
            Connection::UdpListener(socket) => socket,
            Connection::UdpSocket(socket, _) => socket,
        }
    }

    pub fn local_address(&self) -> SocketAddr {
        match self {
            Connection::TcpListener(listener) => listener.local_addr().unwrap(),
            Connection::TcpStream(stream) => stream.local_addr().unwrap(),
            Connection::UdpListener(socket) => socket.local_addr().unwrap(),
            Connection::UdpSocket(socket, _) => socket.local_addr().unwrap(),
        }
    }

    pub fn remote_address(&self) -> Option<SocketAddr> {
        match self {
            Connection::TcpListener(_) => None,
            Connection::TcpStream(stream) => Some(stream.peer_addr().unwrap()),
            Connection::UdpListener(_) => None,
            Connection::UdpSocket(_, address) => Some(*address),
        }
    }
}

pub struct Store {
    connections: HashMap<usize, Connection>,
    last_id: usize,
    registry: Registry,
    virtual_socket_connections_addr_id: HashMap<SocketAddr, usize>,
    virtual_socket_connections_id_addr: HashMap<usize, SocketAddr>,
}

impl Store {
    fn new(registry: Registry) -> Store {
        Store {
            connections: HashMap::new(),
            last_id: 0,
            registry,
            virtual_socket_connections_addr_id: HashMap::new(),
            virtual_socket_connections_id_addr: HashMap::new(),
        }
    }

    pub fn add_connection(&mut self, mut connection: Connection) -> usize {
        let id = self.last_id;
        self.last_id += 1;
        self.registry.register(connection.event_source(), Token(id), Interest::READABLE).unwrap();
        self.connections.insert(id, connection);
        id
    }

    pub fn remove_connection(&mut self, id: usize) -> Option<()> {
        if let Some(ref mut connection) = self.connections.remove(&id) {
            self.registry.deregister(connection.event_source()).unwrap();
            Some(())
        }
        else if let Some(addr) = self.virtual_socket_connections_id_addr.remove(&id) {
            self.virtual_socket_connections_addr_id.remove(&addr);
            Some(())
        }
        else {
            None
        }
    }

    pub fn connection_local_address(&self, id: usize) -> Option<SocketAddr> {
        self.connections.get(&id).map(|connection| connection.local_address())
    }

    pub fn connection_remote_address(&self, id: usize) -> Option<SocketAddr> {
        match self.connections.get(&id) {
            Some(connection) => connection.remote_address(),
            None => None,
        }
    }

    pub fn register_virtual_socket_connection(&mut self, addr: SocketAddr) -> (bool, usize) {
        match self.virtual_socket_connections_addr_id.get(&addr) {
            Some(id) => (false, *id),
            None => {
                let id = self.last_id;
                self.last_id += 1;
                self.virtual_socket_connections_addr_id.insert(addr, id);
                self.virtual_socket_connections_id_addr.insert(id, addr);
                (true, id)
            }
        }
    }
}


pub struct Controller {
    store: Arc<Mutex<Store>>,
}

impl Controller {
    fn new(store: Arc<Mutex<Store>>) -> Controller {
        Controller { store }
    }

    pub fn add_connection(&mut self, connection: Connection) -> usize {
        let mut store = self.store.lock().unwrap();
        store.add_connection(connection)
    }

    pub fn remove_connection(&mut self, id: usize) -> Option<()> {
        let mut store = self.store.lock().unwrap();
        store.remove_connection(id)
    }

    pub fn connection_local_address(&self, id: usize) -> Option<SocketAddr> {
        let store = self.store.lock().unwrap();
        store.connection_local_address(id)
    }

    pub fn connection_remote_address(&self, id: usize) -> Option<SocketAddr> {
        let store = self.store.lock().unwrap();
        store.connection_remote_address(id)
    }

    pub fn send(&mut self, id: usize, data: &[u8]) -> Option<()> {
        let mut store = self.store.lock().unwrap();
        match store.connections.get_mut(&id) {
            Some(connection) => match connection {
                Connection::TcpStream(stream) => { stream.write(data).ok().map(|_|()) }, //TODO: Generate a Disconnection Event if broke pipe
                Connection::UdpSocket(socket, _) => { socket.send(data).ok().map(|_|()) },
                _ => None,
            },
            None => None,
        }
    }
}

impl Clone for Controller {
    fn clone(&self) -> Self {
        Self { store: self.store.clone() }
    }
}

pub struct Receiver {
    store: Arc<Mutex<Store>>,
    poll: Poll,
    events: Events,
    input_buffer: [u8; INPUT_BUFFER_SIZE],
}

impl<'a> Receiver {
    fn new(store: Arc<Mutex<Store>>, poll: Poll) -> Receiver {
        Receiver {
            store,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
            input_buffer: [0; INPUT_BUFFER_SIZE],
        }
    }

    pub fn receive<C>(&mut self, timeout: Option<Duration>, callback: C)
    where C: for<'b> FnMut(&mut Store, usize, Event<'b>) {
        self.poll.poll(&mut self.events, timeout).unwrap();
        if !self.events.is_empty() {
            self.process_event(callback);
        }
    }

    fn process_event<C>(&mut self, mut callback: C)
    where C: for<'b> FnMut(&mut Store, usize, Event<'b>) {
        let mio_event = self.events.iter().next().unwrap();
        match mio_event.token() {
            token => {
                let id = token.0;
                let mut store = self.store.lock().unwrap();
                match store.connections.get_mut(&id).unwrap() {
                    Connection::TcpListener(listener) => {
                        let (stream, addr) = listener.accept().unwrap();
                        let stream_id = store.add_connection(Connection::TcpStream(stream));
                        callback(&mut store, stream_id, Event::Connection(addr));
                    },
                    Connection::TcpStream(ref mut stream) => {
                        if let Ok(size) = stream.read(&mut self.input_buffer) {
                            match size {
                                0 => {
                                    callback(&mut store, id, Event::Disconnection);
                                    store.remove_connection(id);
                                },
                                _ => callback(&mut store, id, Event::Data(&self.input_buffer)),
                            }
                        };
                    },
                    Connection::UdpListener(ref socket) => {
                        if let Ok((_, addr)) = socket.recv_from(&mut self.input_buffer) {
                            let (new, endpoint_id) = store.register_virtual_socket_connection(addr);
                            if new {
                                callback(&mut store, endpoint_id, Event::Connection(addr))
                            }
                            callback(&mut store, endpoint_id, Event::Data(&self.input_buffer))
                        };
                    },
                    Connection::UdpSocket(ref socket, _) => {
                        if let Ok(_) = socket.recv(&mut self.input_buffer) {
                            callback(&mut store, id, Event::Data(&self.input_buffer))
                        };
                    }
                };
            }
        };
    }
}

