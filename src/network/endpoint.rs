use super::resource_id::{ResourceId};

use std::net::{SocketAddr};

/// Information to identify the remote endpoint.
/// The endpoint is used mainly as a connection identified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Endpoint {
    resource_id: ResourceId,
    addr: SocketAddr,
}

impl Endpoint {
    /// Creates a new Endpoint to use in non connection oriented protocols.
    ///
    /// For non connection-oriented protocols, as *UDP*, the endpoint can be created manually
    /// from a listener resource to send messages to different address without creating a connection.
    ///
    /// For connection oriented protocol, to create manually and endpoint is not allowed.
    ///
    /// # Example
    /// ```rust
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::{Transport, Endpoint, NetEvent};
    ///
    /// let (handler, listener) = node::split::<()>();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1)); //timeout
    ///
    /// let listen_addr = "127.0.0.1:0";
    /// let (receiver_id_1, addr_1) = handler.network().listen(Transport::Udp, listen_addr).unwrap();
    /// let (receiver_id_2, addr_2) = handler.network().listen(Transport::Udp, listen_addr).unwrap();
    /// let (sender_id, _) = handler.network().listen(Transport::Udp, listen_addr).unwrap();
    ///
    /// //addr_1 and addr_2 contains the address with the listening port.
    /// handler.network().send(Endpoint::new(sender_id, addr_1), &[23]);
    /// handler.network().send(Endpoint::new(sender_id, addr_2), &[42]);
    ///
    /// let (mut msg_1, mut msg_2) = (0, 0);
    /// listener.for_each(|event| match event {
    ///     NodeEvent::Signal(_) => handler.stop(),
    ///     NodeEvent::Network(net_event) => match net_event {
    ///         NetEvent::Message(endpoint, message) => match endpoint.resource_id() {
    ///             id if id == receiver_id_1 => msg_1 = message[0],
    ///             id if id == receiver_id_2 => msg_2 = message[0],
    ///             _ => unreachable!(),
    ///         }
    ///         _ => unreachable!(),
    ///     }
    /// });
    ///
    /// assert_eq!((msg_1, msg_2), (23, 42));
    /// ```
    pub fn new(resource_id: ResourceId, addr: SocketAddr) -> Self {
        Self { resource_id, addr }
    }

    /// Returns the inner network resource id used by this endpoint.
    /// It is not necessary to be unique for each endpoint if some of them shared the resource
    /// (an example of this is the different endpoints generated by when you listen by udp).
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the peer address of the endpoint.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} {}", self.resource_id, self.addr)
    }
}
