use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

/// Resource is the element used as Remote or Listener into the whole adapter.
/// It usually is a wrapper over a socket.
pub trait Resource: Send + Sync {
    /// This is the only required method to make your element as resource.
    /// Any `mio` net element implements [`Source`].
    /// See [`mio::event::Source`].
    fn source(&mut self) -> &mut dyn Source;
}

/// High level trait to represent an adapter for a transport protocol.
/// The adapter is used to pack an [`ActionHandler`] and an [`EventHandler`],
/// two traits that describes how an adapter behaves.
pub trait Adapter {
    /// Resource type used to identify remote connections and send/receive
    /// from remote this endpoints (e.g. TcpStream)
    /// This can be considerered the resource used for client connections.
    type Remote: Resource;

    /// Resource type used to accept new connections (e.g. TcpListener)
    /// This can be considerered the resource used for server listenings.
    type Listener: Resource;

    /// See `ActionHandler` trait
    type ActionHandler: ActionHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// See `EventHandler` trait
    type EventHandler: EventHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// Creates am [`ActionHandler`] and [`EventHandler`] that represents the adapter.
    /// The **implementator** must create both instances here.
    fn split(self) -> (Self::ActionHandler, Self::EventHandler);
}

/// The following represents the posible status that a `send()`/`send_all()` call can return.
/// The library do not encourage to perform the check of this status for each `send()` call,
/// Only in that cases where you need extra information about how the sending method was.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SendStatus {
    /// This status is received when the entire data has been sent.
    /// It does not guarantees that the packet have been successfully received by the endpoint.
    /// It means that the correspond adapter has sent the message to the OS without errors.
    Sent,

    /// This status is received in datagram-based protocols where there is a limit in the bytes
    /// that a packet can have.
    /// The first value is the length of the data that was attempt to send
    /// and the second one is the maximun offers by the datagram based protocol used.
    MaxPacketSizeExceeded(usize, usize),

    /// It means that the connection is not able for sending the message.
    /// This implies that a [`crate::network::NetEvent::Disconnected`] has been or will be
    /// generated.
    /// The library encourage to manage the disconnection error in the event queue based with
    /// the RemoveEndpoint received, and left this status to determinated in some cases
    /// if the message was not sent.
    ResourceNotFound,
}

/// This entity is in change to perform direct actions from the user to the network.
pub trait ActionHandler: Send {
    type Remote: Resource;
    type Listener: Resource;

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
}

/// Used as a parameter callback in [`crate::adapter::EventHandler::accept_event()`]
pub enum AcceptedType<'a, R> {
    /// The listener has accepted a remote (`R`) the specified addr.
    /// The remote will be registered for generate calls to
    /// [`crate::adapter::EventHandler::read_event()`].
    Remote(SocketAddr, R),

    /// The listener has accepted data that can be packed into a message from a specified addr.
    /// This will produce a `Message` API event.
    Data(SocketAddr, &'a [u8]),
}

/// Returned as a result of [`crate::adapter::EventHandler::read_event()`]
pub enum ReadStatus {
    /// This status must be returned if the resource has been disconnected or there was an error.
    /// The resource will be removed after this call.
    /// No more [`crate::adapter::EventHandler::read_event()`] calls will be produced
    /// by this resource.
    Disconnected,

    /// This status must be returned when a the resource (treated as a non-bloking) would wait for
    /// process the next event.
    /// Usually, this status is returned if the resource returns [`std::io::ErrorKind::WouldBlock`].
    WaitNextEvent,
}

/// This entity is in change to perform eventual actions comming from the internal network engine.
/// The associated methods will generates indirectly the [`crate::network::NetEvent`] to the user.
pub trait EventHandler: Send {
    type Remote: Resource;
    type Listener: Resource;

    /// Called when a listener resource received an event.
    /// It means that some resource have tried to connect.
    /// The **implementator** is in charge of accepting this connection.
    /// The `accept_remote` must be called for each accept request in the listener.
    /// Note that an accept event could imply to process more than one remote.
    /// This function is called when the listener has one or more pending connections.
    fn accept_event(
        &mut self,
        listener: &Self::Listener,
        accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>),
    );

    /// Called when a remote endpoint received an event.
    /// It means that the resource has available data to read,
    /// or there is some connection related issue, as a disconnection.
    /// The **implementator** is in charge of processing that action and returns a [`ReadStatus`].
    /// The `process_data` function must be called for each data chunk that represents a message.
    /// This `process_data` function will produce a `Message` API event.
    /// Note that a read event could imply more than one call to `read`.
    fn read_event(&mut self, remote: &Self::Remote, process_data: &dyn Fn(&[u8])) -> ReadStatus;
}
