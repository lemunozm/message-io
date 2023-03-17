use crate::network::transport::{TransportConnect, TransportListen};

use super::remote_addr::{RemoteAddr};
use super::poll::{Readiness};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

/// High level trait to represent an adapter for a transport protocol.
/// The adapter is only used to identify the resources of your adapter.
pub trait Adapter: Send + Sync {
    /// Resource type used to identify remote connections and send/receive
    /// from remote this endpoints (e.g. TcpStream)
    /// This can be considerered the resource used for client connections.
    type Remote: Remote;

    /// Resource type used to accept new connections (e.g. TcpListener)
    /// This can be considerered the resource used for server listenings.
    type Local: Local<Remote = Self::Remote>;
}

/// A `Resource` is defined as an object that can return a mutable reference to a [`Source`].
/// `Source` is the trait that [`mio`] uses to register in the poll in order to wake up
/// asynchronously from events.
/// Your [`Remote`] and [`Local`] entities must implement `Resource`.
pub trait Resource: Send + Sync {
    /// Returns a mutable reference to the internal `Source`.
    /// Note: All `mio` network element implements [`Source`], you probably wants to use
    /// one of them as a base for your non-blocking transport.
    /// See [`Source`].
    fn source(&mut self) -> &mut dyn Source;
}

/// Plain struct used as a returned value of [`Remote::connect_with()`]
pub struct ConnectionInfo<R: Remote> {
    /// The new created remote resource
    pub remote: R,

    /// Local address of the interal resource used.
    pub local_addr: SocketAddr,

    /// Peer address of the interal resource used.
    pub peer_addr: SocketAddr,
}

/// Plain struct used as a returned value of [`Local::listen_with()`]
pub struct ListeningInfo<L: Local> {
    /// The new created local resource
    pub local: L,

    /// Local address generated after perform the listening action.
    pub local_addr: SocketAddr,
}

/// The following represents the posible status that [`crate::network::NetworkController::send()`]
/// call can return.
/// The library do not encourage to perform the check of this status for each `send()` call,
/// only in that cases where you need extra information about how the sending method was.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SendStatus {
    /// This status is received when the entire data has been sent.
    /// It does not guarantees that the packet have been successfully received by the endpoint.
    /// It means that the correspond adapter has sent the message to the OS without errors.
    Sent,

    /// This status is received in packet-based protocols where there is a limit in the bytes
    /// that a packet can have.
    MaxPacketSizeExceeded,

    /// It means that the message could not be sent by the specified `ResourceId`.
    /// This implies that a [`crate::network::NetEvent::Disconnected`] has happened or that
    /// the resource never existed.
    ResourceNotFound,

    /// The resource can not perform the required send operation.
    /// Usually this is due because it is performing the handshake.
    ResourceNotAvailable,
}

/// Returned as a result of [`Remote::receive()`]
#[derive(Debug)]
pub enum ReadStatus {
    /// This status must be returned if the resource has been disconnected or there was an error.
    /// The resource will be removed after this call and
    /// no more [`Remote::receive()`] calls will be produced by this resource.
    Disconnected,

    /// This status must be returned when a the resource (treated as a non-bloking) would wait for
    /// process the next event.
    /// Usually, this status is returned if the resource receives
    /// a [`std::io::ErrorKind::WouldBlock`].
    WaitNextEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingStatus {
    /// The resource is no longer considered as a pending resource.
    /// It it came from a listener, a [`crate::network::NetEvent::Accepted`] event will be generated.
    /// It it came from a explicit connection, a [`crate::network::NetEvent::Connected`]
    /// with its flag to `true` will be generated.
    /// No more calls to [`Remote::pending()`] will be performed.
    Ready,

    /// The resource needs more data to be considered as established.
    Incomplete,

    /// The resource has not be able to perform the connection.
    /// It it came from a listener, no event will be generated.
    /// It it came from a explicit connection, a [`crate::network::NetEvent::Connected`]
    /// with its flag to `false` will be generated.
    /// No more calls to [`Remote::pending()`] will be performed and the resource will be removed.
    Disconnected,
}

/// The resource used to represent a remote.
/// It usually is a wrapper over a socket/stream.
pub trait Remote: Resource + Sized {
    /// Called when the user performs a connection request to an specific remote address.
    /// The **implementator** is in change of creating the corresponding remote resource.
    /// The [`TransportConnect`] wraps custom transport options for transports that support it. It
    /// is guaranteed by the upper level to be of the variant matching the adapter. Therefore other
    /// variants can be safely ignored.
    /// The [`RemoteAddr`] contains either a [`SocketAddr`] or a [`url::Url`].
    /// It is in charge of deciding what to do in both cases.
    /// It also must return the extracted address as `SocketAddr`.
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>>;

    /// Called when a remote resource received an event.
    /// The resource must be *ready* to receive this call.
    /// It means that it has available data to read,
    /// or there is some connection related issue, as a disconnection.
    /// The **implementator** is in charge of processing that action and returns a [`ReadStatus`].
    ///
    /// The `process_data` function must be called for each data chunk that represents a message.
    /// This call will produce a [`crate::network::NetEvent::Message`] API event.
    /// Note that `receive()` could imply more than one call to `read`.
    /// The implementator must be read all data from the resource.
    /// For most of the cases it means read until the network resource returns `WouldBlock`.
    fn receive(&self, process_data: impl FnMut(&[u8])) -> ReadStatus;

    /// Sends raw data from a resource.
    /// The resource must be *ready* to receive this call.
    /// The **implementator** is in charge to send the entire `data`.
    /// The [`SendStatus`] will contain the status of this attempt.
    fn send(&self, data: &[u8]) -> SendStatus;

    /// Called when a `Remote` is created (explicity of by a listener)
    /// and it is not consider ready yet.
    /// A remote resource **is considered ready** when it is totally connected
    /// and can be used for writing data.
    /// It implies that the user has received the `Connected` or `Accepted` method for that resource.
    ///
    /// This method is in charge to determine if a resource is ready or not.
    /// No `Connected` or `Accepted` events will be generated until this function return
    /// `PendingStatus::Ready`.
    /// The method wil be called several times with different `Readiness` until the **implementator**
    /// returns a `PendingStatus::Ready` or `PendingStatus::Disconnected`.
    fn pending(&self, readiness: Readiness) -> PendingStatus;

    /// The resource is available to write.
    /// It must be *ready* to receive this call.
    /// Here the **implementator** optionally can try to write any pending data.
    /// The return value is an identification of the operation result.
    /// If the method returns `true`, the operation was successful, otherwise, the resource will
    /// be disconnected and removed.
    fn ready_to_write(&self) -> bool {
        true
    }
}

/// Used as a parameter callback in [`Local::accept()`]
pub enum AcceptedType<'a, R> {
    /// The listener has accepted a remote (`R`) with the specified addr.
    /// The remote will be registered in order to generate read events. (calls to
    /// [`Remote::receive()`]).
    /// A [`crate::network::NetEvent::Accepted`] will be generated once this remote resource
    /// is considered *ready*.
    Remote(SocketAddr, R),

    /// The listener has accepted data that can be packed into a message from a specified addr.
    /// Despite of `Remote`, accept as a `Data` will not register any Remote.
    /// This will produce a [`crate::network::NetEvent::Message`] event.
    /// The endpoint of this event will be unique containing the specified addr and the listener
    /// whom generates it.
    Data(SocketAddr, &'a [u8]),
}

/// The resource used to represent a local listener.
/// It usually is a wrapper over a socket/listener.
pub trait Local: Resource + Sized {
    /// The type of the Remote accepted by the [`Self::accept()`] function.
    /// It must be the same as the adapter's `Remote`.
    type Remote: Remote;

    /// Called when the user performs a listening request from an specific address.
    /// The **implementator** is in change of creating the corresponding local resource.
    /// It also must returned the listening address since it could not be the same as param `addr`
    /// (e.g. listening from port `0`).
    /// The [`TransportListen`] wraps custom transport options for transports that support it. It
    /// is guaranteed by the upper level to be of the variant matching the adapter. Therefore other
    /// variants can be safely ignored.
    fn listen_with(config: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>>;

    /// Called when a local resource received an event.
    /// It means that some resource have tried to connect.
    /// The **implementator** is in charge of accepting this connection.
    /// The `accept_remote` must be called for each accept request in the local resource.
    /// Note that an accept event could imply to process more than one remote.
    /// This function is called when the local resource has one or more pending connections.
    /// The **implementator** must process all these pending connections in this call.
    /// For most of the cases it means accept connections until the network
    /// resource returns `WouldBlock`.
    fn accept(&self, accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>));

    /// Sends a raw data from a resource.
    /// Similar to [`Remote::send()`] but the resource that sends the data is a `Local`.
    /// This behaviour usually happens when the transport to implement is not connection oriented.
    ///
    /// The **implementator** must **only** implement this function if the local resource can
    /// also send data.
    fn send_to(&self, _addr: SocketAddr, _data: &[u8]) -> SendStatus {
        panic!("Adapter not configured to send messages directly from the local resource")
    }
}
