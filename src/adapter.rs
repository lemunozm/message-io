use crate::status::{SendStatus, ReadStatus, AcceptStatus};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

/// High level trait to represent an adapter for a transport protocol.
/// The adapter is used to pack an [`ActionHandler`] and an [`EventHandler`],
/// two traits that describes how an adapter behaves.
pub trait Adapter {
    /// Resource type used to send/receive from remote endpoints (e.g. TcpStream)
    type Remote: Source + Send + Sync;

    /// Resource type used to accept new connections (e.g. TcpListener)
    type Listener: Source + Send + Sync;

    /// See `ActionHandler` trait
    type ActionHandler: ActionHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// See `EventHandler` trait
    type EventHandler: EventHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// Creates am [`ActionHandler`] and [`EventHandler`] that represents the adapter.
    /// The **implementator** must create both instances here.
    fn split(self) -> (Self::ActionHandler, Self::EventHandler);
}

/// This entity is in change to perform direct actions from the user to the network.
pub trait ActionHandler: Send {
    type Remote: Source;
    type Listener: Source;

    /// The user performs a connection request to an specific address.
    /// The **implementator** is in change of creating the corresponding remote resource.
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Self::Remote>;

    /// The user performs a listening request from an specific address.
    /// The **implementator** is in change of creating the corresponding listener resource.
    fn listen(&mut self, addr: SocketAddr) -> io::Result<(Self::Listener, SocketAddr)>;

    /// Sends a raw data from a resource.
    /// The **implementator** is in charge to send the `data` using the `resource`.
    /// The [`SendStatus`] will contain the status of this sending attempt.
    fn send(&mut self, resource: &Self::Remote, data: &[u8]) -> SendStatus;

    /// Similar to [`ActionHandler::send()`] but the resource that send the data is a listener.
    /// The **implementator** must **only** implement this if the listener resource can
    /// also send data.
    /// This behaviour usually happens when the transport to implement is not connection oriented.
    /// The param `target_addr` represents the address to send that data.
    fn send_by_listener(
        &mut self,
        _resource: &Self::Listener,
        _target_addr: SocketAddr,
        _data: &[u8],
    ) -> SendStatus
    {
        panic!("Error: You are sending a message from a listener resource");
    }

    /// When a user removes a remote resource it is moved to this method.
    /// The **implementor** can overwrite this empty behaviour to perform some action over
    /// this resource.
    /// The `peer_addr` is given because a disconnected resource could not have it available.
    /// Note that this method is only called when the user explicitally removes the resource,
    /// this method will not be called if a [ReadStatus::Disconnected] event is processed.
    /// If you need to perform some action in this case,
    /// implements into [`EventHandler::read_event`].
    fn remove_remote(&mut self, _resource: Self::Remote, _peer_addr: SocketAddr) {}

    /// Similar to [ActionHandler::remove_remote()] but for a listener resource.
    /// The `local_addr` is given because a disconnected resource could not have it available.
    /// The only way to remove a listener instead of a remote is by an explicit call from the user.
    fn remove_listener(&mut self, _resource: Self::Listener, _local_addr: SocketAddr) {}
}

/// This entity is in change to perform eventual actions comming from the internal network engine.
/// The associated methods will generates indirectly the [`crate::network::NetEvent`] to the user.
pub trait EventHandler: Send {
    type Remote: Source;
    type Listener: Source;

    /// Called when a listener resource received an event.
    /// It means that some resource have tried to connect.
    /// The **implementator** is in charge of accepting this connection and returns [AcceptStatus].
    fn accept_event(&mut self, listener: &Self::Listener) -> AcceptStatus<'_, Self::Remote>;

    /// Called when a remote endpoint received an event.
    /// It means that the resource has available data to read,
    /// or there is some connection related issue, as a disconnection.
    /// The **implementator** is in charge of processing that action and returns a [`ReadStatus`].
    /// The `process_data` function must be called for each data chunk that represents a message.
    /// This `process_data` function will produce a `Message` API event.
    fn read_event(&mut self, remote: &Self::Remote, process_data: &dyn Fn(&[u8])) -> ReadStatus;

    /// This method is called when [`ReadStatus::Disconnected`].
    /// The **implementor** can overwrite this empty behaviour to perform
    /// some action over this resource.
    /// The `peer_addr` is given because a disconnected resource could not have it available.
    /// Note that this method is only called when `Disconnected` event is generated,
    /// not when the user remove explicitelly the resource.
    /// For the last case, use [`ActionHandler::remove_remote`].
    /// The resource returned is already removed from the internal OS registers.
    /// Note that since the listener can only be removed by the user, the `EventHandler` has not
    /// a dedicated method for that. See [ActionHandler::remove_listener].
    fn remove_remote(&mut self, _resource: Self::Remote, _peer_addr: SocketAddr) {}
}
