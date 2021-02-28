use crate::engine::{AdapterLauncher};
use crate::adapters::{
    tcp::{TcpAdapter},
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
    /// Note that as stream protocol, receiving a message from Tcp do not imply to read
    /// the entire message.
    /// If you want a packet based way to send over tcp use [`FramedTcp`]
    Tcp,

    /// Like TCP, but encoded with a slim frame layer to manage the data as sized packets,
    /// not as a stream.
    FramedTcp,

    /// Udp protocol.
    /// Note that UDP is not connection oriented, the packet can be lost or received disordered.
    Udp,

    /// Websocket protocol.
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

    /// Max packet payload size available for each transport
    pub const fn max_payload(self) -> usize {
        const UNLIMIT: usize = usize::MAX;
        match self {
            Self::Tcp => UNLIMIT,
            Self::FramedTcp => framed_tcp::MAX_TCP_PAYLOAD_LEN,
            Self::Udp => udp::MAX_UDP_PAYLOAD_LEN,
            Self::Ws => web_socket::MAX_WS_PAYLOAD_LEN,
        }
    }

    pub const fn is_connection_oriented(self) -> bool {
        match self {
            Transport::Tcp => true,
            Transport::FramedTcp => true,
            Transport::Udp => false,
            Transport::Ws => true,
        }
    }

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
