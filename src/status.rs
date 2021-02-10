use std::net::SocketAddr;

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
    /// This implies that a [crate::network::NetEvent::RemovedEndpoint] has been or will be
    /// generated.
    /// The library encourage to manage the disconnection error in the event queue based with
    /// the RemoveEndpoint received, and left this status to determinated in some cases
    /// if the message was not sent.
    RemovedEndpoint,
}

/// Returned as a result of [`EventHandler::acception_event()`]
pub enum AcceptStatus<'a, R> {
    /// The listener has accepted a remote (`R`) the specified addr.
    /// The remote will be registered for generate calls to [`read_event`].
    AcceptedRemote(SocketAddr, R),

    /// The listener has accepted data that can be packed into a message from a specified addr.
    /// This status will produce a `Message` API event.
    AcceptedData(SocketAddr, &'a [u8]),

    /// This status must be returned when the OS operation over a resource is interrupted.
    /// This usually happens if the resource returns [`std::io::ErrorKind::Interrupted`].
    /// If this status is returned, the acception_event will be process again for the same event.
    Interrupted,

    /// This status must be returned when a the resource (treated as a non-bloking) would wait for
    /// process the next event.
    /// Usually, this status is returned if the resource returns [`std::io::ErrorKind::WouldBlock`].
    WaitNextEvent,
}

/// Returned as a result of [`EventHandler::read_event()`]
pub enum ReadStatus {
    /// This status must be returned if data was read until the end.
    /// It means that the remote could contain more data to read.
    /// Read the entire buffer could mean that maybe there are more bytes.
    MoreData,

    /// This status must be returned if the resource has been disconnected or there was an error.
    /// The resource will be removed after this call.
    /// No more [`read_event`] calls will be produced by this resource.
    Disconnected,

    /// See [`AcceptionStatus::Interrupted`]
    Interrupted,

    /// See [`AcceptionStatus::WaitNextEvent`]
    WaitNextEvent,
}

