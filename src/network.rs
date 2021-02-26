// The ext public uses allows the user to use all this elements as network elements
pub use crate::resource_id::{ResourceId, ResourceType};
pub use crate::endpoint::{Endpoint};
pub use crate::adapter::{SendStatus};
pub use crate::remote_addr::{RemoteAddr, ToRemoteAddr};
pub use crate::transport::{Transport};
pub use crate::driver::{AdapterEvent};

use crate::events::{EventQueue};
use crate::engine::{NetworkEngine, AdapterLauncher};

use strum::{IntoEnumIterator};

use std::net::{SocketAddr, ToSocketAddrs};
use std::io::{self};

/// Input network events.
#[derive(Debug)]
pub enum NetEvent {
    /// Input message received by the network.
    Message(Endpoint, Vec<u8>),

    /// New endpoint has been connected to a listener.
    /// This event will be sent only in connection oriented protocols as [`Transport::Tcp`].
    Connected(Endpoint),

    /// This event is only dispatched when a connection is lost.
    /// Call to [`Network::remove()`] will NOT generate the event.
    /// When this event is received, the resource is considered already removed.
    /// A [`NetEvent::Message`] event will never be generated after this event from the endpoint.
    /// This event will be sent only in connection oriented protocols as [`Transport::Tcp`].
    /// Because `UDP` is not connection oriented, the event can no be detected.
    Disconnected(Endpoint),
}

impl NetEvent {
    /// Created a `NetEvent` from an [`AdapterEvent`].
    pub fn from_adapter(endpoint: Endpoint, adapter_event: AdapterEvent<'_>) -> NetEvent {
        match adapter_event {
            AdapterEvent::Added => NetEvent::Connected(endpoint),
            AdapterEvent::Data(data) => NetEvent::Message(endpoint, data.to_vec()),
            AdapterEvent::Removed => NetEvent::Disconnected(endpoint),
        }
    }
}

/// Network is in charge of managing all the connections transparently.
/// It transforms raw data from the network into message events and vice versa,
/// and manages the different adapters for you.
pub struct Network {
    engine: NetworkEngine,
}

impl Network {
    /// Creates a new `Network` instance.
    /// The user must register an event_callback that can be called each time
    /// an internal adapter generates an event.
    /// This function is used when the user needs to perform some action over the raw data
    /// comming from an adapter, without using a [`EventQueue`].
    /// If you will want to use an `EventQueue` you can use [`Network::split()`] or
    /// [`Network::split_and_map()`]
    pub fn new(event_callback: impl Fn(Endpoint, AdapterEvent) + Send + 'static) -> Network {
        let mut launcher = AdapterLauncher::default();
        Transport::iter().for_each(|transport| transport.mount_adapter(&mut launcher));

        let engine = NetworkEngine::new(launcher, event_callback);

        Network { engine }
    }

    /// Creates a network instance with an associated [`EventQueue`] where the input network
    /// events can be read.
    /// If you want to create a [`EventQueue`] that manages more events than `NetEvent`,
    /// You can create use instead [Network::split_and_map()].
    /// This function shall be used if you only want to manage `NetEvent` in the EventQueue.
    pub fn split() -> (EventQueue<NetEvent>, Network) {
        let mut event_queue = EventQueue::new();
        let sender = event_queue.sender().clone();
        let network = Network::new(move |endpoint, adapter_event| {
            sender.send(NetEvent::from_adapter(endpoint, adapter_event))
        });
        // It is totally crucial to return the network at last element of the tuple
        // in order to be dropped before the event queue.
        (event_queue, network)
    }

    /// Creates a network instance with an associated [`EventQueue`] where the input network
    /// events can be read.
    /// This function, allows to map the [`NetEvent`] to something you use in your application,
    /// allowing to mix the `NetEvent` with your own events.
    pub fn split_and_map<E: Send + 'static>(
        map: impl Fn(NetEvent) -> E + Send + 'static,
    ) -> (EventQueue<E>, Network) {
        let mut event_queue = EventQueue::new();
        let sender = event_queue.sender().clone();
        let network = Network::new(move |endpoint, adapter_event| {
            sender.send(map(NetEvent::from_adapter(endpoint, adapter_event)))
        });
        (event_queue, network)
    }

    /// Creates a connection to the specific address.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached)
    /// the corresponding IO error is returned.
    /// This function blocks until the resource has been connected and is ready to use.
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
    pub fn remove(&mut self, resource_id: ResourceId) -> Option<()> {
        self.engine.remove(resource_id)
    }

    /// Send the data message thought the connection represented by the given endpoint.
    /// The funcion panics if the endpoint do not exists in the [`Network`].
    /// If the endpoint disconnects during the sending, a RemoveEndpoint is generated.
    /// A [`SendStatus`] is returned with the information about the sending.
    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        self.engine.send(endpoint, data)
    }
}
