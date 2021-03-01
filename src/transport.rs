use crate::engine::{AdapterLauncher};
use crate::adapters::{
    tcp::{self, TcpAdapter},
    framed_tcp::{self, FramedTcpAdapter},
    udp::{self, UdpAdapter},
    web_socket::{self, WsAdapter},
};

use num_enum::IntoPrimitive;

use strum::{EnumIter};

/// Enum to identified the underlying transport used.
/// It can be passed to [`crate::network::Network::connect()]` and
/// [`crate::network::Network::listen()`] methods to specify the transport used.
#[derive(IntoPrimitive, EnumIter)]
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Transport {
    /// The TCP stream protocol.
    /// Note that as stream protocol, receiving a message from TCP do not imply to read
    /// the entire message.
    /// If you want a packet based way to send over TCP, use [`FramedTcp`]
    Tcp,

    /// Like TCP, but encoded with a slim frame layer to manage the data as a packet,
    /// not as a stream.
    FramedTcp,

    /// UDP protocol.
    /// Note that UDP is not connection oriented, a packet can be lost or received disordered.
    Udp,

    /// WebSocket protocol.
    Ws,
}

impl Transport {
    /// Associates an adapter.
    /// This method mounts the adapter to be used in the `NetworkEngine`
    pub fn mount_adapter(self, launcher: &mut AdapterLauncher) {
        match self {
            Self::Tcp => launcher.mount(self.id(), TcpAdapter),
            Self::FramedTcp => launcher.mount(self.id(), FramedTcpAdapter),
            Self::Udp => launcher.mount(self.id(), UdpAdapter),
            Self::Ws => launcher.mount(self.id(), WsAdapter),
        };
    }

    /// Max packet payload size available for each transport.
    /// If the protocol is not packet-based (e.g. TCP, that is a stream),
    /// the returned value correspond with the maximum bytes that can produce a read event.
    pub const fn max_message_size(self) -> usize {
        match self {
            Self::Tcp => tcp::INPUT_BUFFER_SIZE,
            Self::FramedTcp => framed_tcp::MAX_TCP_PAYLOAD_LEN,
            Self::Udp => udp::MAX_UDP_PAYLOAD_LEN,
            Self::Ws => web_socket::MAX_WS_PAYLOAD_LEN,
        }
    }

    /// Tell if the transport protocol is a connection oriented protocol.
    /// If it is, `Connection` and `Disconnection` events will be generated.
    pub const fn is_connection_oriented(self) -> bool {
        match self {
            Transport::Tcp => true,
            Transport::FramedTcp => true,
            Transport::Udp => false,
            Transport::Ws => true,
        }
    }

    /// Tell if the transport protocol is a packet based protocol.
    /// It implies that any send call corresponds to a data message event.
    /// The opossite of a packet based is a stream based transport (e.g Tcp).
    /// In this case, reading a data message event do not imply reading the entire message sent.
    /// It is in change of the user to determinate how to read the data.
    pub const fn is_packet_based(self) -> bool {
        match self {
            Transport::Tcp => false,
            Transport::FramedTcp => true,
            Transport::Udp => true,
            Transport::Ws => true,
        }
    }

    /// Returns the adapter id used for this transport.
    /// It is equivalent to the position of the enum starting by 0
    pub fn id(self) -> u8 {
        self.into()
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
