use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, Shutdown, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use std::collections::{HashMap};
use std::io::{self, prelude::*};

const EVENTS_SIZE: usize = 1024;
const INPUT_BUFFER_SIZE: usize = 65536;

pub enum Event<'a> {
    Connection,
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
        match *self {
            Connection::TcpListener(ref mut listener) => listener,
            Connection::TcpStream(ref mut stream) => stream,
            Connection::UdpListener(ref mut socket) => socket,
            Connection::UdpSocket(ref mut socket, _) => socket,
        }
    }
}

struct Store {
    pub connections: HashMap<usize, Connection>,
    pub last_id: usize,
    pub registry: Registry,
    pub virtual_socket_connections_addr_id: HashMap<SocketAddr, usize>,
    pub virtual_socket_connections_id_addr: HashMap<usize, SocketAddr>,
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

    fn add_connection(&mut self, mut connection: Connection) -> usize {
        let id = self.last_id;
        self.last_id += 1;
        self.registry.register(connection.event_source(), Token(id), Interest::READABLE).unwrap();
        self.connections.insert(id, connection);
        id
    }

    fn remove_connection(&mut self, id: usize) {
        if let Some(ref mut connection) = self.connections.remove(&id) {
            self.registry.deregister(connection.event_source()).unwrap();
        }
        else if let Some(addr) = self.virtual_socket_connections_id_addr.remove(&id) {
            self.virtual_socket_connections_addr_id.remove(&addr);
        }
    }

    fn register_virtual_socket_connection(&mut self, addr: SocketAddr) -> (bool, usize) {
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

    pub fn remove_connection(&mut self, id: usize) {
        let mut store = self.store.lock().unwrap();
        store.remove_connection(id)
    }

    pub fn connection_address(&mut self, id: usize) -> Option<SocketAddr> {
        let store = self.store.lock().unwrap();
        store.connections.get(&id).map(|connection| {
            match connection {
                Connection::TcpListener(ref listener) => listener.local_addr().unwrap(),
                Connection::TcpStream(ref stream) => stream.peer_addr().unwrap(),
                Connection::UdpListener(ref socket) => socket.local_addr().unwrap(),
                Connection::UdpSocket(_, address) => *address,
            }
        })
    }

    pub fn send(&mut self, id: usize, data: &[u8]) -> bool {
        let mut store = self.store.lock().unwrap();
        match store.connections.get_mut(&id) {
            Some(connection) => match connection {
                Connection::TcpStream(stream) => { stream.write(data).unwrap(); true },
                Connection::UdpSocket(socket, _) => { socket.send(data).unwrap(); true },
                _ => false,
            },
            None => false,
        }
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
    where C: for<'b> FnMut(usize, Event<'b>) {
        self.poll.poll(&mut self.events, timeout).unwrap();
        if !self.events.is_empty() {
            self.process_event(callback);
        }
    }

    fn process_event<C>(&mut self, mut callback: C)
    where C: for<'b> FnMut(usize, Event<'b>) {
        let mio_event = self.events.iter().next().unwrap();
        match mio_event.token() {
            token => {
                let id = token.0;
                let mut store = self.store.lock().unwrap();
                match store.connections.get_mut(&id).unwrap() {
                    Connection::TcpListener(listener) => {
                        let (stream, _) = listener.accept().unwrap();
                        let stream_id = store.add_connection(Connection::TcpStream(stream));
                        callback(stream_id, Event::Connection);
                    },
                    Connection::TcpStream(ref mut stream) => {
                        if let Ok(size) = stream.read(&mut self.input_buffer) {
                            match size {
                                0 => {
                                    store.remove_connection(id);
                                    callback(id, Event::Disconnection);
                                },
                                _ => callback(id, Event::Data(&self.input_buffer)),
                            }
                        };
                    },
                    Connection::UdpListener(ref socket) => {
                        if let Ok((_, addr)) = socket.recv_from(&mut self.input_buffer) {
                            let (new, endpoint_id) = store.register_virtual_socket_connection(addr);
                            if new {
                                callback(endpoint_id, Event::Connection)
                            }
                            callback(endpoint_id, Event::Data(&self.input_buffer))
                        };
                    },
                    Connection::UdpSocket(ref socket, _) => {
                        if let Ok(_) = socket.recv(&mut self.input_buffer) {
                            callback(id, Event::Data(&self.input_buffer))
                        };
                    }
                };
            }
        };
    }
}

