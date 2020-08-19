use crate::network_adapter::{self, Connection};

use serde::{Serialize, Deserialize};

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::net::{SocketAddr, ToSocketAddrs};
use std::thread::{self, JoinHandle};
use std::time::{Duration};
use std::io::{self};

/// Alias to improve the management of the connection ids.
pub type Endpoint = usize;

const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

/// Input network events.
#[derive(Debug)]
pub enum NetEvent<InMessage>
where InMessage: for<'b> Deserialize<'b> + Send + 'static {
    /// Input message received by the network.
    Message(Endpoint, InMessage),

    /// New endpoint added to a listener.
    /// This event will be sent only in TCP.
    /// It will be sent when a new connection was accepted by the listener.
    AddedEndpoint(Endpoint, SocketAddr),

    /// A connection lost event.
    /// This event is only dispatched when a connection is lost. Call to `remove_endpoint()` will not generate the event.
    /// This event will be sent only in TCP. Because UDP is not connection oriented, the event can no be detected.
    RemovedEndpoint(Endpoint),
}

/// NetworkManager allows to manage the network easier.
/// It is in mainly in charge to transform raw data from the network into message events and vice versa.
pub struct NetworkManager {
    network_event_thread: Option<JoinHandle<()>>,
    network_thread_running: Arc<AtomicBool>,
    network_controller: network_adapter::Controller,
    output_buffer: Vec<u8>,
}

impl<'a> NetworkManager {
    /// Creates a new [NetworkManager].
    /// The user must register an event_callback that can be called each time the network generate and [NetEvent]
    pub fn new<InMessage, C>(event_callback: C) -> NetworkManager
    where InMessage: for<'b> Deserialize<'b> + Send + 'static,
          C: Fn(NetEvent<InMessage>) + Send + 'static {
        let (network_controller, mut network_receiver) = network_adapter::adapter();

        let network_thread_running = Arc::new(AtomicBool::new(true));
        let running = network_thread_running.clone();

        let network_event_thread = thread::Builder::new().name("message-io: network".into()).spawn(move || {
            let timeout = Duration::from_millis(NETWORK_SAMPLING_TIMEOUT);
            while running.load(Ordering::Relaxed) {
                network_receiver.receive(Some(timeout), |addr, endpoint, event| {
                    let net_event = match event {
                        network_adapter::Event::Connection(address) => {
                            log::trace!("Connected endpoint {}", address);
                            NetEvent::AddedEndpoint(endpoint, address)
                        },
                        network_adapter::Event::Data(data) => {
                            log::trace!("Message received from {}", addr);
                            let message: InMessage = bincode::deserialize(&data[..]).unwrap();
                            NetEvent::Message(endpoint, message)
                        },
                        network_adapter::Event::Disconnection => {
                            log::trace!("Disconnected endpoint {}", addr);
                            NetEvent::RemovedEndpoint(endpoint)
                        },
                    };
                    event_callback(net_event);
                });
            }
        }).unwrap();

        NetworkManager {
            network_event_thread: Some(network_event_thread),
            network_thread_running,
            network_controller,
            output_buffer: Vec::new()
        }
    }

    /// Creates a connection to the specific address by TCP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached) an error is returned.
    pub fn connect_tcp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<Endpoint> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.register_connection(Connection::new_tcp_stream(addr))
    }

    /// Creates a connection to the specific address by UDP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If there is an error during the socket creation, an error will be returned.
    pub fn connect_udp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<Endpoint> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.register_connection(Connection::new_udp_socket(addr))
    }

    /// Open a port to listen messages from TCP.
    /// If the port can be opened, an endpoint identifying the listener is returned along with the local address, or an error if not.
    pub fn listen_tcp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<(Endpoint, SocketAddr)> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.register_listener(Connection::new_tcp_listener(addr))
    }

    /// Open a port to listen messages from UDP.
    /// If the port can be opened, an endpoint identifying the listener is returned along with the local address, or an error if not.
    pub fn listen_udp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<(Endpoint, SocketAddr)> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.register_listener(Connection::new_udp_listener(addr))
    }

    /// Open a port to listen messages from UDP in multicast.
    /// If the port can be opened, an endpoint identifying the listener is returned along with the local address, or an error if not.
    pub fn listen_udp_multicast<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<(Endpoint, SocketAddr)> {
        match addr.to_socket_addrs().unwrap().next().unwrap() {
            SocketAddr::V4(addr) => self.register_listener(Connection::new_udp_listener_multicast(addr)),
            _ => panic!("listening for udp multicast is only supported for ipv4 addresses"),
        }
    }

    fn register_connection(&mut self, connection: io::Result<Connection>) -> io::Result<Endpoint> {
        connection
            .map(|connection| {
                self.network_controller.add_connection(connection)
            })
    }

    fn register_listener(&mut self, connection: io::Result<Connection>) -> io::Result<(Endpoint, SocketAddr)> {
        connection
            .map(|connection| {
                let address = connection.local_address();
                let endpoint = self.network_controller.add_connection(connection);
                (endpoint, address)
            })
    }

    /// Remove the endpoint and returns its address.
    /// Returns `None` if the endpoint does not exists.
    pub fn remove_endpoint(&mut self, endpoint: Endpoint) -> Option<SocketAddr> {
        self.network_controller.remove_connection(endpoint)
    }

    /// Retrieve the local address associated to an endpoint, or `None` if the endpoint does not exists.
    pub fn local_address(&mut self, endpoint: Endpoint) -> Option<SocketAddr> {
        self.network_controller.connection_local_address(endpoint)
    }

    /// Retrieve the address associated to an endpoint, or `None` if the endpoint does not exists or the endpoint is a listener.
    pub fn remote_address(&mut self, endpoint: Endpoint) -> Option<SocketAddr> {
        self.network_controller.connection_remote_address(endpoint)
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// If the same message should be sent to different endpoints, use `send_all()` to better performance.
    /// Returns an error if there is an error while sending the message, the endpoint does not exists, or if it is not valid.
    pub fn send<OutMessage>(&mut self, endpoint: Endpoint, message: OutMessage) -> io::Result<()>
    where OutMessage: Serialize {
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        let result = self.network_controller.send(endpoint, &self.output_buffer);
        self.output_buffer.clear();
        if let Ok(_) = result {
            log::trace!("Message sent to {}", self.network_controller.connection_remote_address(endpoint).unwrap());
        }
        result
    }

    /// Serialize and send the message thought the connections represented by the given endpoints.
    /// When there are severals endpoints to send the data, this function is faster than consecutive calls to `send()`
    /// since the serialization is only performed one time for all endpoints.
    /// An list of erroneous endpoints along their errors is returned if there was a problem with some message sent.
    pub fn send_all<'b, OutMessage>(&mut self, endpoints: impl IntoIterator<Item=&'b Endpoint>, message: OutMessage) -> Result<(), Vec<(Endpoint, io::Error)>>
    where OutMessage: Serialize {
        let mut errors = Vec::new();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        for endpoint in endpoints {
            match self.network_controller.send(*endpoint, &self.output_buffer) {
                Ok(_) => log::trace!("Message sent to {}", self.network_controller.connection_remote_address(*endpoint).unwrap()),
                Err(err) => errors.push((*endpoint, err))
            }
        }
        self.output_buffer.clear();
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.network_thread_running.store(false, Ordering::Relaxed);
        self.network_event_thread.take().unwrap().join().unwrap();
    }
}
