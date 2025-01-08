use super::loader::{DriverLoader};

#[cfg(feature = "tcp")]
use crate::adapters::tcp::{TcpAdapter, TcpConnectConfig, TcpListenConfig};
#[cfg(feature = "tcp")]
use crate::adapters::framed_tcp::{FramedTcpAdapter, FramedTcpConnectConfig, FramedTcpListenConfig};
#[cfg(feature = "udp")]
use crate::adapters::udp::{self, UdpAdapter, UdpConnectConfig, UdpListenConfig};
#[cfg(feature = "unixsocket")]
use crate::adapters::unix_socket::{self, UnixSocketConnectConfig, UnixSocketDatagramAdapter, UnixSocketStreamAdapter};
#[cfg(feature = "unixsocket")]
use crate::adapters::unix_socket::UnixSocketListenConfig;
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
    /// Unix Socket protocol (available through the *unixsocket* feature).
    /// To be used on systems that support it.
    #[cfg(feature = "unixsocket")]
    UnixSocketStream,
    #[cfg(feature = "unixsocket")]
    UnixSocketDatagram,
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
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketStream => loader.mount(self.id(), UnixSocketStreamAdapter),
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketDatagram => loader.mount(self.id(), UnixSocketDatagramAdapter),
        };
    }

    /// Maximum theorical packet payload length available for each transport.
    ///
    /// The value returned by this function is the **theorical maximum** and could not be valid for
    /// all networks.
    /// You can ensure your message not exceeds `udp::MAX_INTERNET_PAYLOAD_LEN` in order to be
    /// more cross-platform compatible.
    pub const fn max_message_size(self) -> usize {
        match self {
            #[cfg(feature = "tcp")]
            Self::Tcp => usize::MAX,
            #[cfg(feature = "tcp")]
            Self::FramedTcp => usize::MAX,
            #[cfg(feature = "udp")]
            Self::Udp => udp::MAX_LOCAL_PAYLOAD_LEN,
            #[cfg(feature = "websocket")]
            Self::Ws => ws::MAX_PAYLOAD_LEN,
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketStream => usize::MAX,
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketDatagram => unix_socket::MAX_PAYLOAD_LEN,
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
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketStream => true,
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketDatagram => false,
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
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketStream => false,
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketDatagram => true,
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
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketStream => 4,
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketDatagram => 5,
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
            #[cfg(feature = "unixsocket")]
            4 => Transport::UnixSocketStream,
            #[cfg(feature = "unixsocket")]
            5 => Transport::UnixSocketDatagram,
            _ => panic!("Not available transport"),
        }
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug)]
pub enum TransportConnect {
    #[cfg(feature = "tcp")]
    Tcp(TcpConnectConfig),
    #[cfg(feature = "tcp")]
    FramedTcp(FramedTcpConnectConfig),
    #[cfg(feature = "udp")]
    Udp(UdpConnectConfig),
    #[cfg(feature = "websocket")]
    Ws,
    #[cfg(feature = "unixsocket")]
    UnixSocketStream(UnixSocketConnectConfig),
    #[cfg(feature = "unixsocket")]
    UnixSocketDatagram(UnixSocketConnectConfig),
}

impl TransportConnect {
    pub fn id(&self) -> u8 {
        let transport = match self {
            #[cfg(feature = "tcp")]
            Self::Tcp(_) => Transport::Tcp,
            #[cfg(feature = "tcp")]
            Self::FramedTcp(_) => Transport::FramedTcp,
            #[cfg(feature = "udp")]
            Self::Udp(_) => Transport::Udp,
            #[cfg(feature = "websocket")]
            Self::Ws => Transport::Ws,
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketStream(_) => Transport::UnixSocketStream,
            #[cfg(feature = "unixsocket")]
            Self::UnixSocketDatagram(_) => Transport::UnixSocketDatagram,
        };

        transport.id()
    }
}

impl From<Transport> for TransportConnect {
    fn from(transport: Transport) -> Self {
        match transport {
            #[cfg(feature = "tcp")]
            Transport::Tcp => Self::Tcp(TcpConnectConfig::default()),
            #[cfg(feature = "tcp")]
            Transport::FramedTcp => Self::FramedTcp(FramedTcpConnectConfig::default()),
            #[cfg(feature = "udp")]
            Transport::Udp => Self::Udp(UdpConnectConfig::default()),
            #[cfg(feature = "websocket")]
            Transport::Ws => Self::Ws,
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketStream => Self::UnixSocketStream(UnixSocketConnectConfig::default()),
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketDatagram => Self::UnixSocketDatagram(UnixSocketConnectConfig::default()),
        }
    }
}

#[derive(Debug)]
pub enum TransportListen {
    #[cfg(feature = "tcp")]
    Tcp(TcpListenConfig),
    #[cfg(feature = "tcp")]
    FramedTcp(FramedTcpListenConfig),
    #[cfg(feature = "udp")]
    Udp(UdpListenConfig),
    #[cfg(feature = "websocket")]
    Ws,
    #[cfg(feature = "unixsocket")]
    UnixStreamSocket(UnixSocketListenConfig),
    #[cfg(feature = "unixsocket")]
    UnixDatagramSocket(UnixSocketListenConfig),
}

impl TransportListen {
    pub fn id(&self) -> u8 {
        let transport = match self {
            #[cfg(feature = "tcp")]
            Self::Tcp(_) => Transport::Tcp,
            #[cfg(feature = "tcp")]
            Self::FramedTcp(_) => Transport::FramedTcp,
            #[cfg(feature = "udp")]
            Self::Udp(_) => Transport::Udp,
            #[cfg(feature = "websocket")]
            Self::Ws => Transport::Ws,
            #[cfg(feature = "unixsocket")]
            Self::UnixStreamSocket(_) => Transport::UnixSocketStream,
            #[cfg(feature = "unixsocket")]
            Self::UnixDatagramSocket(_) => Transport::UnixSocketDatagram,
        };

        transport.id()
    }
}

impl From<Transport> for TransportListen {
    fn from(transport: Transport) -> Self {
        match transport {
            #[cfg(feature = "tcp")]
            Transport::Tcp => Self::Tcp(TcpListenConfig::default()),
            #[cfg(feature = "tcp")]
            Transport::FramedTcp => Self::FramedTcp(FramedTcpListenConfig::default()),
            #[cfg(feature = "udp")]
            Transport::Udp => Self::Udp(UdpListenConfig::default()),
            #[cfg(feature = "websocket")]
            Transport::Ws => Self::Ws,
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketStream => Self::UnixStreamSocket(UnixSocketListenConfig::default()),
            #[cfg(feature = "unixsocket")]
            Transport::UnixSocketDatagram => Self::UnixDatagramSocket(UnixSocketListenConfig::default()),
        }
    }
}
