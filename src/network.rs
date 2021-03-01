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

impl From<AdapterEvent<'_>> for NetEvent {
    /// Created a `NetEvent` from an [`AdapterEvent`].
    fn from(adapter_event: AdapterEvent<'_>) -> NetEvent {
        match adapter_event {
            AdapterEvent::Added(endpoint) => NetEvent::Connected(endpoint),
            AdapterEvent::Data(endpoint, data) => NetEvent::Message(endpoint, data.to_vec()),
            AdapterEvent::Removed(endpoint) => NetEvent::Disconnected(endpoint),
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
    /// If you will want to use an `EventQueue` you can use [`Network::split()`],
    /// [`Network::split_and_map()`] or [`Network::split_and_map_from_adapter()`] functions.
    pub fn new(event_callback: impl Fn(AdapterEvent) + Send + 'static) -> Network {
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
        let network = Network::new(move |adapter_event| {
            sender.send(NetEvent::from(adapter_event))
        });
        // It is totally crucial to return the network at last element of the tuple
        // in order to be dropped before the event queue.
        (event_queue, network)
    }

    /// Creates a network instance with an associated [`EventQueue`] where the input network
    /// events can be read.
    /// This function, allows to map the [`NetEvent`] to something you use in your application,
    /// allowing to mix the `NetEvent` with your own events.
    /// The map function is computed by the internal read thread.
    /// It is not recomended to make expensive computations inside this map function to not blocks
    /// the internal jobs.
    pub fn split_and_map<E: Send + 'static>(
        map: impl Fn(NetEvent) -> E + Send + 'static,
    ) -> (EventQueue<E>, Network) {
        let mut event_queue = EventQueue::new();
        let sender = event_queue.sender().clone();
        let network = Network::new(move |adapter_event| {
            sender.send(map(NetEvent::from(adapter_event)))
        });
        (event_queue, network)
    }

    /// Creates a network instance with an associated [`EventQueue`] where the input network
    /// events can be read.
    /// This function, allows to map an [`AdapterEvent`] and its associated [`Endpoint`]
    /// to something you use in your application, allowing to mix the data comming from the adapter
    /// with your own events.
    /// As difference from [`Network::split_and_map`] where the `NetEvent` parameter
    /// is already a sendable object, this funcion avoid an internal copy in the received data
    /// giving the reference to the internal data of the adapter (which are not 'sendable').
    /// It is in change of the user to map this data into something 'sendable'.
    /// This funcion can be useful if you want to deserialize the data to something sendable,
    /// avoiding a useless copy.
    /// It is not recomended to make expensive computations inside this map function to not blocks
    /// the internal jobs.
    pub fn split_and_map_from_adapter<E: Send + 'static>(
        map: impl Fn(AdapterEvent<'_>) -> E + Send + 'static,
    ) -> (EventQueue<E>, Network) {
        let mut event_queue = EventQueue::new();
        let sender = event_queue.sender().clone();
        let network =
            Network::new(move |adapter_event| sender.send(map(adapter_event)));
        (event_queue, network)
    }

    /// Creates a connection to the specific address.
    /// The endpoint, an identifier of the new connection, will be returned.
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
    /// when a `0` port is specified, the OS will give choose the value.
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
    /// This is used to remove resources that the program has created explicitely,
    /// as connection or listeners.
    /// Resources of endpoints generated by listening in connection oriented transports
    /// can also be removed to close the connection.
    /// Note that non-oriented connections as UDP use its listener resource to manage all
    /// remote endpoints internally, the remotes have not resource for themselfs.
    /// It means that all generated `Endpoint`s share the `ResourceId` of the listener and
    /// if you remove this resource you are removing the listener of all of them.
    /// For that cases there is no need to remove the resource because non-oriented connections
    /// have not connection itself to close, 'there is no spoon'.
    pub fn remove(&mut self, resource_id: ResourceId) -> Option<()> {
        self.engine.remove(resource_id)
    }

    /// Send the data message thought the connection represented by the given endpoint.
    /// The funcion panics if the endpoint do not exists in the [`Network`].
    /// If the endpoint disconnects during the sending, a `Disconnected` event is generated.
    /// A [`SendStatus`] is returned with the information about the sending.
    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        self.engine.send(endpoint, data)
    }
}
