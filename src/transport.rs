use crate::engine::{AdapterLauncher};

#[cfg(feature = "tcp")] use crate::adapters::tcp::{self, TcpAdapter};
#[cfg(feature = "tcp")] use crate::adapters::framed_tcp::{self, FramedTcpAdapter};

#[cfg(feature = "udp")] use crate::adapters::udp::{self, UdpAdapter};

#[cfg(feature = "websocket")] use crate::adapters::web_socket::{self, WsAdapter};

use strum::{EnumIter};

/// Enum to identified the underlying transport used.
/// It can be passed to [`crate::network::Network::connect()`] and
/// [`crate::network::Network::listen()`] methods to specify the transport used.
#[derive(EnumIter)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Transport {
    /// TCP protocol (available through the *tcp* feature).
    /// As stream protocol, receiving a message from TCP do not imply to read
    /// the entire message.
    /// If you want a packet based way to send over TCP, use `FramedTcp` instead.
    #[cfg(feature = "tcp")] Tcp,

    /// Like TCP, but encoded with a slim frame layer to manage the data as a packet,
    /// instead of as a stream (available through the *tcp* feature).
    /// Most of the time you would want to use this instead of the raw `Tcp`.
    #[cfg(feature = "tcp")] FramedTcp,

    /// UDP protocol (available through the *udp* feature).
    /// Take into account that UDP is not connection oriented and a packet can be lost
    /// or received disordered.
    /// If it is specified in the listener and the address is a Ipv4 in the range of multicast ips
    /// (from `224.0.0.0` to `239.255.255.255`), the listener will be configured in multicast mode.
    #[cfg(feature = "udp")] Udp,

    /// WebSocket protocol (available through the *websocket* feature).
    /// If you use a [`crate::network::RemoteAddr::Url`] in the `connect()` method,
    /// you can specify `wss` of `ws` schemas to connect with or without security.
    /// If you use a [`crate::network::RemoteAddr::SocketAddr`] the socket will be a normal
    /// websocket with the following uri: `ws://{SocketAddr}/message-io-default`.
    #[cfg(feature = "websocket")] Ws,
}

impl Transport {
    /// Associates an adapter.
    /// This method mounts the adapter to be used in the `Network`.
    pub fn mount_adapter(self, launcher: &mut AdapterLauncher) {
        match self {
            #[cfg(feature = "tcp")] Self::Tcp => launcher.mount(self.id(), TcpAdapter),
            #[cfg(feature = "tcp")] Self::FramedTcp => launcher.mount(self.id(), FramedTcpAdapter),
            #[cfg(feature = "udp")] Self::Udp => launcher.mount(self.id(), UdpAdapter),
            #[cfg(feature = "websocket")] Self::Ws => launcher.mount(self.id(), WsAdapter),
        };
    }

    /// Max packet payload size available for each transport.
    /// If the protocol is not packet-based (e.g. TCP, that is a stream),
    /// the returned value correspond with the maximum bytes that can produce a read event.
    pub const fn max_message_size(self) -> usize {
        match self {
            #[cfg(feature = "tcp")] Self::Tcp => tcp::INPUT_BUFFER_SIZE,
            #[cfg(feature = "tcp")] Self::FramedTcp => framed_tcp::MAX_TCP_PAYLOAD_LEN,
            #[cfg(feature = "udp")] Self::Udp => udp::MAX_UDP_PAYLOAD_LEN,
            #[cfg(feature = "websocket")] Self::Ws => web_socket::MAX_WS_PAYLOAD_LEN,
        }
    }

    /// Tell if the transport protocol is a connection oriented protocol.
    /// If it is, `Connection` and `Disconnection` events will be generated.
    pub const fn is_connection_oriented(self) -> bool {
        match self {
            #[cfg(feature = "tcp")] Transport::Tcp => true,
            #[cfg(feature = "tcp")] Transport::FramedTcp => true,
            #[cfg(feature = "udp")] Transport::Udp => false,
            #[cfg(feature = "websocket")] Transport::Ws => true,
        }
    }

    /// Tell if the transport protocol is a packet based protocol.
    /// It implies that any send call corresponds to a data message event.
    /// The opossite of a packet based is a stream based transport (e.g Tcp).
    /// In this case, reading a data message event do not imply reading the entire message sent.
    /// It is in change of the user to determinate how to read the data.
    pub const fn is_packet_based(self) -> bool {
        match self {
            #[cfg(feature = "tcp")] Transport::Tcp => false,
            #[cfg(feature = "tcp")] Transport::FramedTcp => true,
            #[cfg(feature = "udp")] Transport::Udp => true,
            #[cfg(feature = "websocket")] Transport::Ws => true,
        }
    }

    /// Returns the adapter id used for this transport.
    /// It is equivalent to the position of the enum starting by 0
    pub fn id(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
