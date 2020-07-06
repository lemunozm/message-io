use mio::net::{TcpListener, TcpStream};
use mio::{Poll, Interest, Token, Events, event, Registry};

use std::sync::{Arc, Mutex};

use std::net::{SocketAddr};
use std::time::{Duration};
use std::collections::{HashMap};
use std::io::{self};

const EVENTS_SIZE: usize = 128;
const INITIAL_BUFFER_SIZE: usize = 1024;

pub enum Event<'a> {
    Connection,
    Data(&'a[u8]),
    Disconnection,
}

pub fn adapter() -> (Controller, Receiver) {
    let store = Arc::new(Mutex::new(Store {
        endpoints: HashMap::new(),
        last_id: 0,
    }));

    let poll = Poll::new().unwrap();
    let registry = poll.registry().try_clone().unwrap();

    (Controller::new(store.clone(), registry), Receiver::new(store, poll))
}

pub enum Connection {
    Listener(TcpListener),
    Stream(TcpStream),
    //Socket(UdpSocket),
}

impl Connection {
    pub fn new_tcp_stream(addr: SocketAddr) -> io::Result<Connection> {
        TcpStream::connect(addr).map(|stream| Connection::Stream(stream))
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
}

struct Store {
    pub endpoints: HashMap<usize, Connection>,
    pub last_id: usize,
}

pub struct Controller {
    store: Arc<Mutex<Store>>,
    registry: Registry,
}

impl Controller {
    fn new(store: Arc<Mutex<Store>>, registry: Registry) -> Controller {
        Controller { store, registry }
    }

    pub fn add_endpoint(&mut self, mut endpoint: Connection) -> usize {
        let mut store = self.store.lock().unwrap();
        let id = store.last_id;
        self.registry.register(endpoint.event_source(), Token(id), Interest::READABLE).unwrap();
        store.endpoints.insert(id, endpoint);
        id
    }

    pub fn remove_endpoint(&mut self, id: usize) {
        let mut store = self.store.lock().unwrap();
        let mut endpoint = store.endpoints.remove(&id).unwrap();
        self.registry.deregister(endpoint.event_source()).unwrap();
    }
}

pub struct Receiver {
    store: Arc<Mutex<Store>>,
    poll: Poll,
    events: Events,
}

impl<'a> Receiver {
    fn new(store: Arc<Mutex<Store>>, poll: Poll) -> Receiver {
        Receiver {
            store,
            poll,
            events: Events::with_capacity(EVENTS_SIZE),
        }
    }

    pub fn receive(&mut self) -> (usize, Event<'a>) {
        self.receive_timeout(Duration::from_secs(0)).unwrap()
    }

    pub fn receive_timeout(&mut self, timeout: Duration) -> Option<(usize, Event<'a>)> {
        self.poll.poll(&mut self.events, Some(timeout)).unwrap();
        self.events.iter().next().map(|event| {
            match event.token() {
                token => {
                    let id = token.0;
                    let store = self.store.lock().unwrap();
                    match store.endpoints.get(&id).unwrap() {
                        Connection::Listener(listener) => {
                            listener.accept();
                            Some((id, Event::Connection))
                        },
                        Connection::Stream(stream) => {
                            Some((id, Event::Connection))
                        }
                    };
                }
            }
        });
        None
    }
}

