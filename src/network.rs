use crate::network_adapter::{self, Connection};

use serde::{Serialize, Deserialize};

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::net::{SocketAddr};
use std::thread::{self, JoinHandle};
use std::fmt::{self};
use std::time::{Duration};

/// Alias to improve the management of the connection ids.
pub type Endpoint = usize;

const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

/// Used to define the protocol when a connection/listener is created.
#[derive(Debug, Clone, Copy)]
pub enum TransportProtocol {
    Tcp,
    Udp,
}

/// Input network events.
pub enum NetEvent<InMessage>
where InMessage: for<'b> Deserialize<'b> + Send + 'static {
    /// Input message received by the network.
    Message(InMessage, Endpoint),

    /// New endpoint added to a listener.
    /// In TCP it will be sent when a new connection was accepted by the listener.
    /// IN UDP will be sent when the socket send data by first time, before the Message event.
    AddedEndpoint(Endpoint, SocketAddr),

    /// A connection lost event.
    /// This event is only dispatched when a connection is lost, `remove_endpoint()` not generate the event.
    /// This event will be sent only in TCP. Because UDP is not connection oriented, this event can no be detected
    RemovedEndpoint(Endpoint),
}


impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
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

        let network_event_thread = thread::spawn(move || {
            let timeout = Duration::from_millis(NETWORK_SAMPLING_TIMEOUT);
            while running.load(Ordering::Relaxed) {
                network_receiver.receive(Some(timeout), |store, endpoint, event| {
                    let net_event = match event {
                        network_adapter::Event::Connection(address) => {
                            log::trace!("Connected endpoint {}", address);
                            NetEvent::AddedEndpoint(endpoint, address)
                        },
                        network_adapter::Event::Data(data) => {
                            log::trace!("Message received from {}", store.connection_remote_address(endpoint).unwrap());
                            let message: InMessage = bincode::deserialize(&data[..]).unwrap();
                            NetEvent::Message(message, endpoint)
                        },
                        network_adapter::Event::Disconnection => {
                            log::trace!("Disconnected endpoint {}", store.connection_remote_address(endpoint).unwrap());
                            NetEvent::RemovedEndpoint(endpoint)
                        },
                    };
                    event_callback(net_event);
                });
            }
        });

        NetworkManager {
            network_event_thread: Some(network_event_thread),
            network_thread_running,
            network_controller,
            output_buffer: Vec::new()
        }
    }

    /// Creates a connection to the specific address thougth the given protocol.
    /// The endpoint, an identified of the new connection, will be returned among with the local address of that connection.
    /// Only for TCP, if the connection can not be performed (the address is not reached) a None will be returned.
    /// UDP has no the ability to be aware of this, so always an endpoint will be returned.
    pub fn connect(&mut self, addr: SocketAddr, transport: TransportProtocol) -> Option<(Endpoint, SocketAddr)> {
        match transport {
            TransportProtocol::Tcp => Connection::new_tcp_stream(addr),
            TransportProtocol::Udp => Connection::new_udp_socket(addr),
        }
        .ok()
        .map(|connection| {
            let address = connection.local_address();
            let endpoint = self.network_controller.add_connection(connection);
            (endpoint, address)
        })
    }

    /// Open a port to listen messages from either TCP or UDP.
    /// If the port can be opened, a endpoint identifying the listener will be returned among with the local address, or a None if not.
    pub fn listen(&mut self, addr: SocketAddr, transport: TransportProtocol) -> Option<(Endpoint, SocketAddr)> {
        match transport {
            TransportProtocol::Tcp => Connection::new_tcp_listener(addr),
            TransportProtocol::Udp => Connection::new_udp_listener(addr),
        }
        .ok()
        .map(|connection| {
            let address = connection.local_address();
            let endpoint = self.network_controller.add_connection(connection);
            (endpoint, address)
        })
    }

    /// Retrieve the local address associated to an endpoint, or None if the endpoint does not exists.
    pub fn endpoint_local_address(&mut self, endpoint: Endpoint) -> Option<SocketAddr> {
        self.network_controller.connection_local_address(endpoint)
    }

    /// Retrieve the address associated to an endpoint, or None if the endpoint does not exists or the endpoint is a listener.
    pub fn endpoint_remote_address(&mut self, endpoint: Endpoint) -> Option<SocketAddr> {
        self.network_controller.connection_remote_address(endpoint)
    }

    /// Remove the endpoint.
    /// Returns None if the endpoint does not exists.
    pub fn remove_endpoint(&mut self, endpoint: Endpoint) -> Option<()> {
        self.network_controller.remove_connection(endpoint)
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// Returns None if the endpoint does not exists.
    pub fn send<OutMessage>(&mut self, endpoint: Endpoint, message: OutMessage) -> Option<()>
    where OutMessage: Serialize {
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        let result = self.network_controller.send(endpoint, &self.output_buffer);
        self.output_buffer.clear();
        if let Some(_) = result {
            log::trace!("Message sent to {}", self.network_controller.connection_remote_address(endpoint).unwrap());
        }
        result
    }

    /// Serialize and send the message thought the connections represented by the given endpoints.
    /// When there are severals endpoints to send the data. It is better to call this function instead of several calls to `send()`,
    /// because the serialization only is performed one time for all the endpoints.
    /// An Err with the unrecognized ids is returned.
    pub fn send_all<'b, OutMessage>(&mut self, endpoints: impl IntoIterator<Item=&'b Endpoint>, message: OutMessage) -> Result<(), Vec<Endpoint>>
    where OutMessage: Serialize {
        let mut unrecognized_ids = Vec::new();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        for endpoint in endpoints {
            match self.network_controller.send(*endpoint, &self.output_buffer) {
                Some(_) => log::trace!("Message sent to {}", self.network_controller.connection_remote_address(*endpoint).unwrap()),
                None => unrecognized_ids.push(*endpoint)
            }
        }
        self.output_buffer.clear();
        if unrecognized_ids.is_empty() { Ok(()) } else { Err(unrecognized_ids) }
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.network_thread_running.store(false, Ordering::Relaxed);
        self.network_event_thread.take().unwrap().join().unwrap();
    }
}
