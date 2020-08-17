use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use std::collections::{HashMap};
use std::io::{self, prelude::*, ErrorKind};

const EVENTS_SIZE: usize = 1024;
const INPUT_BUFFER_SIZE: usize = 65536;

#[derive(Debug)]
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
        // Create a standard tcpstream to blocking until the connection is reached.
        StdTcpStream::connect(addr).map(|stream| {
            stream.set_nonblocking(true).unwrap();
            Connection::TcpStream(TcpStream::from_std(stream))
        })
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

    pub fn tcp_listener(&mut self) -> &mut TcpListener {
        match self {
            Connection::TcpListener(listener) => listener,
            _ => panic!(),
        }
    }

    pub fn udp_listener(&mut self) -> &mut UdpSocket {
        match self {
            Connection::UdpListener(socket) => socket,
            _ => panic!(),
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
                Connection::TcpStream(stream) => { stream.write(data).ok().map(|_|()) },
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
    where C: for<'b> FnMut(SocketAddr, usize, Event<'b>) {
        loop {
            match self.poll.poll(&mut self.events, timeout) {
                Ok(_) => {
                    break self.process_event(callback)
                },
                Err(e) => match e.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => Err(e).unwrap(),
                }
            }
        }
    }

    fn process_event<C>(&mut self, mut callback: C)
    where C: for<'b> FnMut(SocketAddr, usize, Event<'b>) {
        for mio_event in &self.events {
            match mio_event.token() {
                token => {
                    let id = token.0;
                    let mut store = self.store.lock().unwrap();

                    let connection = store.connections.get_mut(&id).unwrap();
                    match connection {
                        Connection::TcpListener(listener) => {
                            log::trace!("Wake from poll for endpoint {}: TcpListener", id);
                            let mut listener = listener;
                            loop {
                                match listener.accept() {
                                    Ok((stream, addr)) => {
                                        let stream_id = store.add_connection(Connection::TcpStream(stream));
                                        callback(addr, stream_id, Event::Connection(addr));
                                        listener = store.connections.get_mut(&id).unwrap().tcp_listener();
                                    }
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        },
                        Connection::TcpStream(stream) => {
                            log::trace!("Wake from poll for endpoint {}: TcpStream", id);
                            loop {
                                match stream.read(&mut self.input_buffer) {
                                    Ok(0) => {
                                        callback(stream.peer_addr().unwrap(), id, Event::Disconnection);
                                        store.remove_connection(id);
                                        break;
                                    },
                                    Ok(_) => {
                                        callback(stream.peer_addr().unwrap(), id, Event::Data(&self.input_buffer));
                                    }
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        },
                        Connection::UdpListener(socket) => {
                            log::trace!("Wake from poll for endpoint {}: UdpListener", id);
                            let mut socket = socket;
                            loop {
                                match socket.recv_from(&mut self.input_buffer) {
                                    Ok((_, addr)) => {
                                        let (_, endpoint_id) = store.register_virtual_socket_connection(addr);
                                        callback(addr, endpoint_id, Event::Data(&self.input_buffer));
                                        socket = store.connections.get_mut(&id).unwrap().udp_listener();
                                    },
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        },
                        Connection::UdpSocket(socket, addr) => {
                            log::trace!("Wake from poll for endpoint {}: UdpSocket", id);
                            loop {
                                match socket.recv(&mut self.input_buffer) {
                                    Ok(_) => callback(*addr, id, Event::Data(&self.input_buffer)),
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        }
                    };
                }
            }
        };
    }
}

