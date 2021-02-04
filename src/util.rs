pub const OTHER_THREAD_ERR: &str = "This error is shown because other thread has panicked";

/// The following represents the posible status that a `send()`/`send_all()` call can return.
/// The library do not encourage to perform the match of this status for each `send()` call,
/// Only in that cases where you need extra information about how the sending method was.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SendingStatus {
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
