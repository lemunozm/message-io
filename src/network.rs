use crate::network_adapter::{self, Controller, Listener, Remote};
use crate::encoding::{self, DecodingPool};

use serde::{Serialize, Deserialize};

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::net::{SocketAddr, ToSocketAddrs};
use std::thread::{self, JoinHandle};
use std::time::{Duration};
use std::io::{self};

pub use crate::network_adapter::{Endpoint, MAX_UDP_LEN};

const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const INPUT_BUFFER_SIZE: usize = 65536;

/// Input network events.
#[derive(Debug)]
pub enum NetEvent<InMessage>
where InMessage: for<'b> Deserialize<'b> + Send + 'static
{
    /// Input message received by the network.
    Message(Endpoint, InMessage),

    /// New endpoint added to a listener.
    /// It will be sent when a new connection was accepted by the listener.
    /// This event will be sent only in TCP.
    AddedEndpoint(Endpoint),

    /// A connection lost event.
    /// This event is only dispatched when a connection is lost. Call to `remove_resource()` will not generate the event.
    /// After this event, the resource is considered removed.
    /// A Message event will never be generated after this event.
    /// This event will be sent only in TCP. Because UDP is not connection oriented, the event can no be detected.
    RemovedEndpoint(Endpoint),
}

/// NetworkManager allows to manage the network easier.
/// It is in mainly in charge to transform raw data from the network into message events and vice versa.
pub struct NetworkManager {
    network_event_thread: Option<JoinHandle<()>>,
    network_thread_running: Arc<AtomicBool>,
    network_controller: Arc<Mutex<Controller>>,
    output_buffer: Vec<u8>,
}

impl<'a> NetworkManager {
    /// Creates a new [NetworkManager].
    /// The user must register an event_callback that can be called each time the network generate and [NetEvent]
    pub fn new<InMessage, C>(event_callback: C) -> NetworkManager
    where
        InMessage: for<'b> Deserialize<'b> + Send + 'static,
        C: Fn(NetEvent<InMessage>) + Send + 'static,
    {
        let (network_controller, mut network_receiver) = network_adapter::adapter();

        let network_thread_running = Arc::new(AtomicBool::new(true));
        let running = network_thread_running.clone();

        let network_event_thread = thread::Builder::new()
            .name("message-io: network".into())
            .spawn(move || {
                let mut input_buffer = [0; INPUT_BUFFER_SIZE];
                let mut decoding_pool = DecodingPool::new();
                let timeout = Duration::from_millis(NETWORK_SAMPLING_TIMEOUT);
                while running.load(Ordering::Relaxed) {
                    network_receiver.receive(
                        &mut input_buffer[..],
                        Some(timeout),
                        |endpoint, event| {
                            Self::process_network_event(
                                &event_callback,
                                endpoint,
                                event,
                                &mut decoding_pool,
                            );
                        },
                    );
                }
            })
            .unwrap();

        NetworkManager {
            network_event_thread: Some(network_event_thread),
            network_thread_running,
            network_controller,
            output_buffer: Vec::new(),
        }
    }

    fn process_network_event<'c, InMessage, C>(
        event_callback: &C,
        endpoint: Endpoint,
        event: network_adapter::Event<'c>,
        decoding_pool: &mut DecodingPool<Endpoint>,
    ) where
        InMessage: for<'b> Deserialize<'b> + Send + 'static,
        C: Fn(NetEvent<InMessage>) + Send + 'static,
    {
        match event {
            network_adapter::Event::Connection => {
                log::trace!("Connected endpoint {}", endpoint);
                event_callback(NetEvent::AddedEndpoint(endpoint));
            }
            network_adapter::Event::Data(data) => {
                log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                decoding_pool.decode_from(data, endpoint, |decoded_data| {
                    log::trace!("Message received from {}, {} bytes", endpoint, decoded_data.len());
                    let message: InMessage = bincode::deserialize(decoded_data).unwrap();
                    event_callback(NetEvent::Message(endpoint, message));
                });
            }
            network_adapter::Event::Disconnection => {
                log::trace!("Disconnected endpoint {}", endpoint);
                decoding_pool.remove_if_exists(endpoint);
                event_callback(NetEvent::RemovedEndpoint(endpoint));
            }
        };
    }

    /// Creates a connection to the specific address by TCP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached) an error is returned.
    pub fn connect_tcp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<Endpoint> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        Remote::new_tcp(addr)
            .map(|remote| self.network_controller.lock().unwrap().add_remote(remote))
    }

    /// Creates a connection to the specific address by UDP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If there is an error during the socket creation, an error will be returned.
    pub fn connect_udp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<Endpoint> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        Remote::new_udp(addr)
            .map(|remote| self.network_controller.lock().unwrap().add_remote(remote))
    }

    /// Open a port to listen messages from TCP.
    /// If the port can be opened, an resource id identifying the listener is returned along with the local address, or an error if not.
    pub fn listen_tcp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<(usize, SocketAddr)> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        Listener::new_tcp(addr)
            .map(|listener| self.network_controller.lock().unwrap().add_listener(listener))
    }

    /// Open a port to listen messages from UDP.
    /// If the port can be opened, an resource id identifying the listener is returned along with the local address, or an error if not.
    pub fn listen_udp<A: ToSocketAddrs>(&mut self, addr: A) -> io::Result<(usize, SocketAddr)> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        Listener::new_udp(addr)
            .map(|listener| self.network_controller.lock().unwrap().add_listener(listener))
    }

    /// Open a port to listen messages from UDP in multicast.
    /// If the port can be opened, an resource id identifying the listener is returned along with the local address, or an error if not.
    /// Only ipv4 addresses are allowed.
    pub fn listen_udp_multicast<A: ToSocketAddrs>(
        &mut self,
        addr: A,
    ) -> io::Result<(usize, SocketAddr)>
    {
        match addr.to_socket_addrs().unwrap().next().unwrap() {
            SocketAddr::V4(addr) => Listener::new_udp_multicast(addr)
                .map(|listener| self.network_controller.lock().unwrap().add_listener(listener)),
            _ => panic!("listening for udp multicast is only supported for ipv4 addresses"),
        }
    }

    /// Remove a network resource.
    /// Returns `None` if the resource id not exists.
    /// This is used mainly to remove resources that the program has been created explicitely, as connection or listeners.
    /// Resources of endpoints generated by a TcpListener can also be removed to close the connection.
    /// Note: Udp endpoints generated by a UdpListener shared the resource, the own UdpListener.
    /// This means that there is no resource to remove as the TCP case.
    pub fn remove_resource(&mut self, resource_id: usize) -> Option<()> {
        self.network_controller.lock().unwrap().remove_resource(resource_id)
    }

    /// Request a local address of a resource.
    /// Returns `None` if the endpoint id not exists.
    /// Note: Udp endpoints generated by a UdpListener shared the resource.
    pub fn local_address(&self, resource_id: usize) -> Option<SocketAddr> {
        self.network_controller.lock().unwrap().local_address(resource_id)
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// If the same message should be sent to different endpoints, use `send_all()` to better performance.
    /// Returns an error if there is an error while sending the message, the endpoint does not exists, or if it is not valid.
    pub fn send<OutMessage>(&mut self, endpoint: Endpoint, message: OutMessage) -> io::Result<()>
    where OutMessage: Serialize {
        self.prepare_output_message(message);
        let result = self.network_controller.lock().unwrap().send(endpoint, &self.output_buffer);
        self.output_buffer.clear();
        if result.is_ok() {
            log::trace!("Message sent to {}", endpoint);
        }
        result
    }

    /// Serialize and send the message thought the connections represented by the given endpoints.
    /// When there are severals endpoints to send the data, this function is faster than consecutive calls to `send()`
    /// since the serialization is only performed one time for all endpoints.
    /// A list of erroneous endpoints along with their errors is returned if there was a problem with some messages sent.
    /// An error sending a message by an endpoint do not avoid sending the message to the rest of endpoints.
    pub fn send_all<'b, OutMessage>(
        &mut self,
        endpoints: impl IntoIterator<Item = &'b Endpoint>,
        message: OutMessage,
    ) -> Result<(), Vec<(Endpoint, io::Error)>>
    where
        OutMessage: Serialize,
    {
        self.prepare_output_message(message);
        let mut errors = Vec::new();
        let mut controller = self.network_controller.lock().unwrap();
        for endpoint in endpoints {
            match controller.send(*endpoint, &self.output_buffer) {
                Ok(_) => log::trace!("Message sent to {}", endpoint),
                Err(err) => errors.push((*endpoint, err)),
            }
        }
        self.output_buffer.clear();
        if errors.is_empty() {
            Ok(())
        }
        else {
            Err(errors)
        }
    }

    fn prepare_output_message<OutMessage>(&mut self, message: OutMessage)
    where OutMessage: Serialize {
        encoding::encode(&mut self.output_buffer, |enconding_slot| {
            bincode::serialize_into(enconding_slot, &message).unwrap();
        });
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.network_thread_running.store(false, Ordering::Relaxed);
        self.network_event_thread.take().unwrap().join().unwrap();
    }
}
