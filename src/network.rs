pub use crate::resource_id::{ResourceId, ResourceType};
pub use crate::endpoint::{Endpoint};
pub use crate::adapter::{SendStatus};
pub use crate::remote_addr::{RemoteAddr, ToRemoteAddr};
pub use crate::transport::{Transport};

use crate::events::{EventQueue};
use crate::engine::{NetworkEngine, AdapterLauncher};
use crate::driver::{AdapterEvent};

use serde::{Serialize, Deserialize};

use std::net::{SocketAddr, ToSocketAddrs};
use std::io::{self};

/// Input network events.
#[derive(Debug)]
pub enum NetEvent<M>
where M: for<'b> Deserialize<'b> + Send + 'static
{
    /// Input message received by the network.
    Message(Endpoint, M),

    /// New endpoint has been connected to a listener.
    /// This event will be sent only in connection oriented protocols as [`Transport::Tcp`].
    Connected(Endpoint),

    /// This event is only dispatched when a connection is lost.
    /// Call to [`Network::remove_resource()`] will NOT generate the event.
    /// When this event is received, the resource is considered already removed.
    /// A [`NetEvent::Message`] event will never be generated after this event from the endpoint.
    /// This event will be sent only in connection oriented protocols as [`Transport::Tcp`].
    /// Because `UDP` is not connection oriented, the event can no be detected.
    Disconnected(Endpoint),

    /// This event shows that there was a problem during the deserialization of a message.
    /// The problem is mainly due by a programming issue reading data from
    /// an unknown or outdated endpoint.
    /// In production it could be that other application is writing in your application port.
    /// This error means that a message has been lost (the erroneous message),
    /// but the endpoint remains connected for its usage.
    DeserializationError(Endpoint),
}

/// Network is in charge of managing all the connections transparently.
/// It transforms raw data from the network into message events and vice versa,
/// and manages the different adapters for you.
pub struct Network {
    engine: NetworkEngine,
    output_buffer: Vec<u8>, //cached for preformance
}

impl Network {
    /// Creates a new `Network` instance.
    /// The user must register an event_callback that can be called
    /// each time the network generate and [`NetEvent`].
    pub fn new<M, C>(event_callback: C) -> Network
    where
        M: for<'b> Deserialize<'b> + Send + 'static,
        C: Fn(NetEvent<M>) + Send + 'static,
    {
        let mut launcher = AdapterLauncher::default();
        Transport::mount_all(&mut launcher);

        let engine = NetworkEngine::new(launcher, move |endpoint, adapter_event| {
            let event = match adapter_event {
                AdapterEvent::Added => {
                    log::trace!("Endpoint connected: {}", endpoint);
                    NetEvent::Connected(endpoint)
                }
                AdapterEvent::Data(data) => {
                    log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                    match bincode::deserialize::<M>(data) {
                        Ok(message) => NetEvent::Message(endpoint, message),
                        Err(_) => NetEvent::DeserializationError(endpoint),
                    }
                }
                AdapterEvent::Removed => {
                    log::trace!("Endpoint disconnected: {}", endpoint);
                    NetEvent::Disconnected(endpoint)
                }
            };
            event_callback(event);
        });

        Network { engine, output_buffer: Vec::new() }
    }

    /// Creates a network instance with an associated [`EventQueue`] where the input network
    /// events can be read.
    /// If you want to create a [`EventQueue`] that manages more events than `NetEvent`,
    /// Yoy can create a custom network with [Network::new()].
    /// This function shall be used if you only want to manage `NetEvent` in the EventQueue.
    pub fn split<M>() -> (Network, EventQueue<NetEvent<M>>)
    where M: for<'b> Deserialize<'b> + Send + 'static {
        let mut event_queue = EventQueue::new();

        let sender = event_queue.sender().clone();
        let network = Network::new(move |net_event| sender.send(net_event));
        (network, event_queue)
    }

    /// Creates a connection to the specific address.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached)
    /// the corresponding IO error is returned.
    pub fn connect(
        &mut self,
        transport: Transport,
        addr: impl ToRemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)>
    {
        let addr = addr.to_remote_addr().unwrap();
        self.engine.connect(transport.id(), addr)
    }

    /// Listen messages from specified transport.
    /// The giver address will be used as interface and listening port.
    /// If the port can be opened, a [ResourceId] identifying the listener is returned
    /// along with the local address, or an error if not.
    /// The address is returned despite you passed as parameter because
    /// when a `0` port is specified, the OS will give a value.
    /// If the protocol is UDP and the address is Ipv4 in the range of multicast ips
    /// (from `224.0.0.0` to `239.255.255.255`) it will be listening is multicast mode.
    pub fn listen(
        &mut self,
        transport: Transport,
        addr: impl ToSocketAddrs,
    ) -> io::Result<(ResourceId, SocketAddr)>
    {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.engine.listen(transport.id(), addr)
    }

    /// Remove a network resource.
    /// Returns `None` if the resource id doesn't exists.
    /// This is used to remove resources that the program has been created explicitely,
    /// as connection or listeners.
    /// Resources of endpoints generated by listening in connection oriented transports
    /// can also be removed to close the connection.
    /// Note: UDP endpoints generated by listening from UDP shared the resource.
    /// This means that there is no resource to remove because there is no connection itself
    /// to close ('there is no spoon').
    pub fn remove_resource(&mut self, resource_id: ResourceId) -> Option<()> {
        self.engine.remove(resource_id)
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// If the same message should be sent to different endpoints,
    /// use [`Network::send_all()`] to get a better performance.
    /// The funcion panics if the endpoint do not exists in the [`Network`].
    /// If the endpoint disconnects during the sending, a RemoveEndpoint is generated.
    /// A [`SendStatus`] is returned with the information about the sending.
    pub fn send<M: Serialize>(&mut self, endpoint: Endpoint, message: M) -> SendStatus {
        self.output_buffer.clear();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();

        let status = self.engine.send(endpoint, &self.output_buffer);
        log::trace!("Message sent to {}, {:?}", endpoint, status);
        status
    }

    /// This method performs the same actions as [`Network::send()`] but for several endpoints.
    /// When there are severals endpoints to send the data,
    /// this method is faster than consecutive calls to [`Network::send()`]
    /// since the encoding and serialization is performed only one time for all endpoints.
    /// The method returns a list of [`SendStatus`] associated to each endpoint.
    pub fn send_all<'b, M: Serialize>(
        &mut self,
        endpoints: impl IntoIterator<Item = &'b Endpoint>,
        message: M,
    ) -> &Vec<SendStatus>
    {
        self.output_buffer.clear();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        self.engine.send_all(endpoints, &self.output_buffer)
    }
}
