use mio::net::{TcpListener, TcpStream, UdpSocket};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr, TcpStream as StdTcpStream};
use net2::{UdpBuilder};
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use std::collections::{HashMap, hash_map::Entry};
use std::io::{self, prelude::*, ErrorKind};

const EVENTS_SIZE: usize = 1024;
const INPUT_BUFFER_SIZE: usize = 65536;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Endpoint {
    connection_id: usize,
    addr: SocketAddr,
}

impl Endpoint {
    fn new(connection_id: usize, addr: SocketAddr) -> Endpoint {
        Endpoint { connection_id, addr }
    }

    pub fn connection_id(&self) -> usize {
        self.connection_id
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

#[derive(Debug)]
pub enum Event<'a> {
    Connection,
    Data(&'a[u8]),
    Disconnection,
}

pub fn adapter() -> (Arc<Mutex<Controller>>, Receiver) {
    let poll = Poll::new().unwrap();
    let controller = Controller::new(poll.registry().try_clone().unwrap());
    let thread_safe_controller = Arc::new(Mutex::new(controller));
    (thread_safe_controller.clone(), Receiver::new(thread_safe_controller, poll))
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

    pub fn new_udp_listener_multicast(addr: SocketAddrV4) -> io::Result<Connection> {
        let listening_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port());
        UdpBuilder::new_v4().unwrap().reuse_address(true).unwrap().bind(listening_addr).map(|socket| {
            socket.join_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
            Connection::UdpListener(UdpSocket::from_std(socket))
        })
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

    pub fn addr(&self) -> SocketAddr {
        match self {
            Connection::TcpListener(listener) => listener.local_addr().unwrap(),
            Connection::TcpStream(stream) => stream.peer_addr().unwrap(),
            Connection::UdpListener(socket) => socket.local_addr().unwrap(),
            Connection::UdpSocket(_, addr) => *addr,
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
}

impl Drop for Connection {
    fn drop(&mut self) {
        match self {
            Connection::UdpListener(socket) => {
                if let SocketAddr::V4(addr) = socket.local_addr().unwrap() {
                    if addr.ip().is_multicast() {
                        socket.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
                    }
                }
            },
            _ => (),
        }
    }
}

pub struct Controller {
    connections: HashMap<usize, Connection>,
    last_id: usize,
    registry: Registry,
}

impl Controller {
    fn new(registry: Registry) -> Controller {
        Controller {
            connections: HashMap::new(),
            last_id: 0,
            registry,
        }
    }

    pub fn add_connection(&mut self, mut connection: Connection) -> Endpoint {
        let id = self.last_id;
        self.last_id += 1;
        let endpoint = Endpoint::new(id, connection.addr());
        self.registry.register(connection.event_source(), Token(id), Interest::READABLE).unwrap();
        self.connections.insert(id, connection);
        endpoint
    }

    pub fn remove_connection(&mut self, endpoint: Endpoint) -> Option<()> {
        match self.connections.entry(endpoint.connection_id()) {
            Entry::Occupied(mut entry) => {
                let connection = entry.get_mut();
                if connection.addr() != endpoint.addr() {
                    // If address differs means that it is a virtual connection created by the UdpListener.
                    // Virtual connections are not registered into the registry or the connections.
                    panic!("Endpoint {} is a virtual UDP connection. It can not be removed because not exists.");
                }
                self.registry.deregister(connection.event_source()).unwrap();
                entry.remove_entry();
                Some(())
            },
            Entry::Vacant(_) => None,
        }
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> io::Result<()> {
        if let Some(connection) = self.connections.get_mut(&endpoint.connection_id()) {
            match connection {
                Connection::TcpStream(stream) => stream.write(data).map(|_|()),
                Connection::UdpSocket(socket, _) => socket.send(data).map(|_|()),
                Connection::UdpListener(socket) => socket.send_to(data, endpoint.addr()).map(|_|()),
                _ => Err(io::Error::new(ErrorKind::PermissionDenied, "TCP Listener connections can not send data")),
            }
        }
        else {
            Err(io::Error::new(ErrorKind::NotFound, "Connection id not exists in the network adapter"))
        }
    }
}

pub struct Receiver {
    controller: Arc<Mutex<Controller>>,
    poll: Poll,
    events: Events,
    input_buffer: [u8; INPUT_BUFFER_SIZE],
}

impl<'a> Receiver {
    fn new(controller: Arc<Mutex<Controller>>, poll: Poll) -> Receiver {
        Receiver {
            controller,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
            input_buffer: [0; INPUT_BUFFER_SIZE],
        }
    }

    pub fn receive<C>(&mut self, timeout: Option<Duration>, callback: C)
    where C: for<'b> FnMut(Endpoint, Event<'b>) {
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
    where C: for<'b> FnMut(Endpoint, Event<'b>) {
        for mio_event in &self.events {
            match mio_event.token() {
                token => {
                    let id = token.0;
                    let mut controller = self.controller.lock().unwrap();

                    let connection = controller.connections.get_mut(&id).unwrap();
                    match connection {
                        Connection::TcpListener(listener) => {
                            log::trace!("Wake from poll for endpoint {}: TcpListener", id);
                            let mut listener = listener;
                            loop {
                                match listener.accept() {
                                    Ok((stream, _)) => {
                                        let endpoint = controller.add_connection(Connection::TcpStream(stream));
                                        callback(endpoint, Event::Connection);

                                        // Used to avoid the consecutive mutable borrows
                                        listener = match controller.connections.get_mut(&id).unwrap() {
                                            Connection::TcpListener(listener) => listener,
                                            _ => unreachable!(),
                                        }
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
                                        let endpoint = Endpoint::new(id, stream.peer_addr().unwrap());
                                        callback(endpoint, Event::Disconnection);
                                        controller.remove_connection(endpoint).unwrap();
                                        break;
                                    },
                                    Ok(_) => {
                                        let endpoint = Endpoint::new(id, stream.peer_addr().unwrap());
                                        callback(endpoint, Event::Data(&self.input_buffer));
                                    },
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        },
                        Connection::UdpListener(socket) => {
                            log::trace!("Wake from poll for endpoint {}: UdpListener", id);
                            loop {
                                match socket.recv_from(&mut self.input_buffer) {
                                    Ok((_, addr)) => callback(Endpoint::new(id, addr), Event::Data(&self.input_buffer)),
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(err) => Err(err).unwrap(),
                                }
                            }
                        },
                        Connection::UdpSocket(socket, addr) => {
                            log::trace!("Wake from poll for endpoint {}: UdpSocket", id);
                            loop {
                                match socket.recv(&mut self.input_buffer) {
                                    Ok(_) => callback(Endpoint::new(id, *addr), Event::Data(&self.input_buffer)),
                                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                                    Err(ref err) if err.kind() == io::ErrorKind::ConnectionRefused => continue,
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

