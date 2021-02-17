use crate::endpoint::{Endpoint};
use crate::engine::{AdapterLauncher};
use crate::driver::{AdapterEvent};
use crate::adapters::{
    tcp::{TcpAdapter},
    udp::{UdpAdapter},
    web_socket::{WsAdapter},
};

use num_enum::IntoPrimitive;

use strum::{IntoEnumIterator, EnumIter};

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
    /// Returns the adapter id used for this transport.
    /// It is equivalent to the position of the enum starting by 0
    pub fn id(self) -> u8 {
        self.into()
    }

    /// Associates an adapter.
    /// This method mounts the adapter to be used in the `NetworkEngine`
    fn mount_adapter<C>(self, launcher: &mut AdapterLauncher<C>)
    where C: Fn(Endpoint, AdapterEvent<'_>) + Send + 'static {
        match self {
            Transport::Tcp => launcher.mount(self.id(), TcpAdapter),
            Transport::Udp => launcher.mount(self.id(), UdpAdapter),
            Transport::Ws => launcher.mount(self.id(), WsAdapter),
        };
    }

    pub(crate) fn mount_all<C>(launcher: &mut AdapterLauncher<C>)
    where C: Fn(Endpoint, AdapterEvent<'_>) + Send + 'static {
        Transport::iter().for_each(|transport| transport.mount_adapter(launcher));
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
