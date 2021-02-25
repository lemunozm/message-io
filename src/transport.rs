use crate::engine::{AdapterLauncher};
use crate::adapters::{
    tcp::{self, TcpAdapter},
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
    Tcp,
    Udp,
    Ws,
}

impl Transport {
    /// Associates an adapter.
    /// This method mounts the adapter to be used in the `NetworkEngine`
    pub fn mount_adapter(self, launcher: &mut AdapterLauncher) {
        match self {
            Self::Tcp => launcher.mount(self.id(), TcpAdapter),
            Self::Udp => launcher.mount(self.id(), UdpAdapter),
            Self::Ws => launcher.mount(self.id(), WsAdapter),
        };
    }

    /// Max packet payload size available for each transport
    pub const fn max_payload(self) -> usize {
        match self {
            Self::Tcp => tcp::MAX_TCP_PAYLOAD_LEN,
            Self::Udp => udp::MAX_UDP_PAYLOAD_LEN,
            Self::Ws => web_socket::MAX_WS_PAYLOAD_LEN,
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
