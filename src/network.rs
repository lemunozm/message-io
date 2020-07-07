use mio::net::{TcpListener, TcpStream};
use mio::{event, Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr, Shutdown, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use std::collections::{HashMap};
use std::io::{self, prelude::*};

const EVENTS_SIZE: usize = 128;
const INPUT_BUFFER_SIZE: usize = 1024;

pub enum Event<'a> {
    Connection,
    Data(&'a[u8]),
    Disconnection,
}

pub fn adapter() -> (Controller, Receiver) {
    let poll = Poll::new().unwrap();
    let store = Arc::new(Mutex::new(Store {
        connections: HashMap::new(),
        last_id: 0,
        registry: poll.registry().try_clone().unwrap(),
    }));

    (Controller::new(store.clone()), Receiver::new(store, poll))
}

pub enum Connection {
    Listener(TcpListener),
    Stream(TcpStream),
    //Socket(UdpSocket),
}

impl Connection {
    pub fn new_tcp_stream(addr: SocketAddr) -> io::Result<Connection> {
        StdTcpStream::connect(addr).map(|stream| Connection::Stream(TcpStream::from_std(stream)))
    }

    pub fn new_tcp_listener(addr: SocketAddr) -> io::Result<Connection> {
        TcpListener::bind(addr).map(|listener| Connection::Listener(listener))
    }

    pub fn event_source(&mut self) -> &mut dyn event::Source {
        match *self {
            Connection::Listener(ref mut listener) => listener,
            Connection::Stream(ref mut stream) => stream,
        }
    }

    pub fn address(&self) -> SocketAddr {
        match *self {
            Connection::Listener(ref listener) => listener.local_addr().unwrap(),
            Connection::Stream(ref stream) => stream.peer_addr().unwrap(),
        }
    }
}

struct Store {
    pub connections: HashMap<usize, Connection>,
    pub last_id: usize,
    pub registry: Registry,
}

impl Store {
    fn add_connection(&mut self, mut connection: Connection) -> usize {
        let id = self.last_id;
        self.last_id += 1;
        self.registry.register(connection.event_source(), Token(id), Interest::READABLE).unwrap();
        self.connections.insert(id, connection);
        id
    }

    fn remove_connection(&mut self, id: usize) {
        let mut connection = self.connections.remove(&id).unwrap();
        match &connection {
            Connection::Stream(stream) => stream.shutdown(Shutdown::Both).unwrap(),
            _ => (),
        };
        self.registry.deregister(connection.event_source()).unwrap();
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

    pub fn connection_address(&mut self, connection_id: usize) -> Option<SocketAddr> {
        let store = self.store.lock().unwrap();
        store.connections.get(&connection_id).map(|connection| connection.address())
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

    pub fn receive<C>(&mut self, callback: C)
    where C: for <'b> FnMut(usize, Event<'b>) {
        self.poll.poll(&mut self.events, None).unwrap();
        self.process_event(callback);
    }

    pub fn receive_timeout<C>(&mut self, timeout: Duration, callback: C)
    where C: for<'b> FnMut(usize, Event<'b>) {
        if let Ok(_) = self.poll.poll(&mut self.events, Some(timeout)) {
            self.process_event(callback);
        }
    }

    fn process_event<C>(&mut self, mut callback: C)
    where C: for<'b> FnMut(usize, Event<'b>) {
        let net_event = self.events.iter().next().unwrap();
        match net_event.token() {
            token => {
                let id = token.0;
                let mut store = self.store.lock().unwrap();
                match store.connections.get_mut(&id).unwrap() {
                    Connection::Listener(listener) => {
                        let (stream, _) = listener.accept().unwrap();
                        let stream_id = store.add_connection(Connection::Stream(stream));
                        callback(stream_id, Event::Connection);
                    },
                    Connection::Stream(ref mut stream) => {
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
                };
            }
        };
    }
}

