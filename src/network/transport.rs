use super::loader::{DriverLoader};
use super::resource_id::{ResourceId};

#[cfg(feature = "tcp")]
use crate::adapters::tcp::{TcpAdapter};
#[cfg(feature = "tcp")]
use crate::adapters::framed_tcp::{FramedTcpAdapter};
#[cfg(feature = "udp")]
use crate::adapters::udp::{self, UdpAdapter};
#[cfg(feature = "websocket")]
use crate::adapters::ws::{self, WsAdapter};

use serde::{Serialize, Deserialize};

/// Enum to identified the underlying transport used.
/// It can be passed to
/// [`NetworkController::connect()`](crate::network::NetworkController::connect()) and
/// [`NetworkController::listen()`](crate::network::NetworkController::connect()) methods
/// to specify the transport used.
#[derive(strum::EnumIter, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Transport {
    /// TCP protocol (available through the *tcp* feature).
    /// As stream protocol, receiving a message from TCP do not imply to read
    /// the entire message.
    /// If you want a packet based way to send over TCP, use `FramedTcp` instead.
    #[cfg(feature = "tcp")]
    Tcp,

    /// Tcp framed protocol (available through the *tcp* feature).
    /// Like TCP, but encoded with a slim frame layer to manage the data as a packet,
    /// instead of as a stream.
    /// It prefixes the message using variable integer encoding with the size of the message.
    /// Most of the time you would want to use this instead of the raw `Tcp`.
    #[cfg(feature = "tcp")]
    FramedTcp,

    /// UDP protocol (available through the *udp* feature).
    /// Take into account that UDP is not connection oriented and a packet can be lost
    /// or received disordered.
    /// If it is specified in the listener and the address is a Ipv4 in the range of multicast ips
    /// (from `224.0.0.0` to `239.255.255.255`), the listener will be configured in multicast mode.
    #[cfg(feature = "udp")]
    Udp,

    /// WebSocket protocol (available through the *websocket* feature).
    /// If you use a [`crate::network::RemoteAddr::Str`] in the `connect()` method,
    /// you can specify an URL with `wss` of `ws` schemas to connect with or without security.
    /// If you use a [`crate::network::RemoteAddr::Socket`] the socket will be a normal
    /// websocket with the following uri: `ws://{SocketAddr}/message-io-default`.
    #[cfg(feature = "websocket")]
    Ws,
}

impl Transport {
    /// Associates an adapter.
    /// This method mounts the adapters to be used in the network instance.
    pub fn mount_adapter(self, loader: &mut DriverLoader) {
        match self {
            #[cfg(feature = "tcp")]
            Self::Tcp => loader.mount(self.id(), TcpAdapter),
            #[cfg(feature = "tcp")]
            Self::FramedTcp => loader.mount(self.id(), FramedTcpAdapter),
            #[cfg(feature = "udp")]
            Self::Udp => loader.mount(self.id(), UdpAdapter),
            #[cfg(feature = "websocket")]
            Self::Ws => loader.mount(self.id(), WsAdapter),
        };
    }

    /// Maximum teorical packet payload length available for each transport.
    ///
    /// Note: For UDP this value *depends* of the OS.
    /// The value returned by this function is the **teorical maximum** and could not be valid for
    /// all OS (*MacOS* in as an example of this).
    /// You can ensure your message not exceeds `udp::MAX_COMPATIBLE_UDP_PAYLOAD_LEN` in order to be
    /// more cross-platform compatible.
    pub const fn max_message_size(self) -> usize {
        match self {
            #[cfg(feature = "tcp")]
            Self::Tcp => usize::MAX,
            #[cfg(feature = "tcp")]
            Self::FramedTcp => usize::MAX,
            #[cfg(feature = "udp")]
            Self::Udp => udp::MAX_PAYLOAD_LEN,
            #[cfg(feature = "websocket")]
            Self::Ws => ws::MAX_PAYLOAD_LEN,
        }
    }

    /// Tell if the transport protocol is a connection oriented protocol.
    /// If it is, `Connection` and `Disconnection` events will be generated.
    pub const fn is_connection_oriented(self) -> bool {
        match self {
            #[cfg(feature = "tcp")]
            Transport::Tcp => true,
            #[cfg(feature = "tcp")]
            Transport::FramedTcp => true,
            #[cfg(feature = "udp")]
            Transport::Udp => false,
            #[cfg(feature = "websocket")]
            Transport::Ws => true,
        }
    }

    /// Tell if the transport protocol is a packet-based protocol.
    /// It implies that any send call corresponds to a data message event.
    /// It satisfies that the number of bytes sent are the same as received.
    /// The opossite of a packet-based is a stream-based transport (e.g Tcp).
    /// In this case, reading a data message event do not imply reading the entire message sent.
    /// It is in change of the user to determinate how to read the data.
    pub const fn is_packet_based(self) -> bool {
        match self {
            #[cfg(feature = "tcp")]
            Transport::Tcp => false,
            #[cfg(feature = "tcp")]
            Transport::FramedTcp => true,
            #[cfg(feature = "udp")]
            Transport::Udp => true,
            #[cfg(feature = "websocket")]
            Transport::Ws => true,
        }
    }

    /// Returns the adapter id used for this transport.
    /// It is equivalent to the position of the enum starting by 0
    pub const fn id(self) -> u8 {
        match self {
            #[cfg(feature = "tcp")]
            Transport::Tcp => 0,
            #[cfg(feature = "tcp")]
            Transport::FramedTcp => 1,
            #[cfg(feature = "udp")]
            Transport::Udp => 2,
            #[cfg(feature = "websocket")]
            Transport::Ws => 3,
        }
    }
}

impl From<u8> for Transport {
    fn from(id: u8) -> Self {
        match id {
            #[cfg(feature = "tcp")]
            0 => Transport::Tcp,
            #[cfg(feature = "tcp")]
            1 => Transport::FramedTcp,
            #[cfg(feature = "udp")]
            2 => Transport::Udp,
            #[cfg(feature = "websocket")]
            3 => Transport::Ws,
            _ => panic!("Not available transport"),
        }
    }
}

impl From<ResourceId> for Transport {
    fn from(id: ResourceId) -> Self {
        id.adapter_id().into()
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use strum::{IntoEnumIterator};

    #[test]
    fn id_equivalence() {
        Transport::iter().for_each(|transport| {
            assert!(transport == Transport::from(transport.id()));
        });
    }
}
