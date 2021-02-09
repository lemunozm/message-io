use crate::util::{SendingStatus};

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

    /// See trait ActionHandler
    type ActionHandler: ActionHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// See trait EventHandler
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
    /// The [`SendingStatus`] will contain the status of this sending attempt.
    fn send(&mut self, resource: &Self::Remote, data: &[u8]) -> SendingStatus;

    /// Similar to [`ActionHandler::Send()`] but the resource that send the data is a listener.
    /// The **implementator** must **only** implement this if the listener resource can
    /// also send data.
    /// This behaviour usually happens when the transport to implement is not connection oriented.
    /// The param `target_addr` represents the address to send that data.
    fn send_by_listener(
        &mut self,
        _resource: &Self::Listener,
        _target_addr: SocketAddr,
        _data: &[u8],
    ) -> SendingStatus
    {
        panic!("Error: You are sending a message from a listener resource");
    }

    /// A remote resource that has been removed from the registers is moved to this function.
    /// The user can overwrite this empty behaviour to perform some action of this resource.
    /// The `peer_addr` is given because a disconnected resource could not have it available.
    fn remove_remote(&mut self, _resource: Self::Remote, _peer_addr: SocketAddr) {}

    /// Similar to [ActionHandler::remove_remote()] but for a listener resource.
    /// The `local_addr` is given because a disconnected resource could not have it available.
    fn remove_listener(&mut self, _resource: Self::Listener, _local_addr: SocketAddr) {}
}

/// Used by [EventHandler::accept_event()]
pub enum AcceptionEvent<'a, R> {
    Remote(SocketAddr, R),
    Data(SocketAddr, &'a [u8]),
}

/*
/// Used by [EventHandler::accept_event()]
pub enum ReadEvent<'a, R> {
    Data(&'a [u8]),
    Disconnected,
}
*/

/// This entity is in change to perform eventual actions comming from the internal network engine.
/// The associated function will generates indirectly the [create::network::NetEvent] to the user.
pub trait EventHandler: Send {
    type Remote: Source;
    type Listener: Source;

    /// Called when a listener resource received an event.
    /// It means that some resource have tried to connect.
    /// The **implementator** is in charge of accepting this connection and returns [AcceptionEvent].
    fn acception_event(
        &mut self,
        listener: &Self::Listener,
        event_callback: &mut dyn Fn(AcceptionEvent<'_, Self::Remote>),
    );

    /// Called when a remote endpoint received an event.
    /// It means that the resource has available data to read,
    /// or there is some connection related issue, as a disconnection.
    /// The **implementator** is in charge of processing that action and returns a [`ReadEvent`].
    fn read_event(
        &mut self,
        remote: &Self::Remote,
        addr: SocketAddr,
        event_callback: &mut dyn Fn(&[u8]),
    ) -> bool;
}
