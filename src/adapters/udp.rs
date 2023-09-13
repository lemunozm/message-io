use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};

use mio::net::{UdpSocket};
use mio::event::{Source};

use socket2::{Socket, Domain, Type, Protocol};

#[cfg(target_os = "linux")]
use nix::errno::{Errno};
#[cfg(target_os = "linux")]
use nix::sys::socket::{self, sockopt, MsgFlags, SockaddrStorage, ControlMessageOwned};
#[cfg(target_os = "linux")]
use nix::ifaddrs::{getifaddrs};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
#[cfg(target_os = "linux")]
use std::net::{IpAddr, Ipv6Addr};
use std::io::{self, ErrorKind};
use std::mem::{MaybeUninit};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

/// Maximun payload that UDP can send over the internet to be mostly compatible.
pub const MAX_INTERNET_PAYLOAD_LEN: usize = 1500 - 20 - 8;
// - 20: max IP header
// - 8: max udp header

/// Similar to [`MAX_INTERNET_PAYLOAD_LEN`] but for localhost instead of internet.
/// Localhost can handle a bigger MTU.
#[cfg(not(target_os = "macos"))]
pub const MAX_LOCAL_PAYLOAD_LEN: usize = 65535 - 20 - 8;

#[cfg(target_os = "macos")]
pub const MAX_LOCAL_PAYLOAD_LEN: usize = 9216 - 20 - 8;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UdpConnectConfig {
    source_address: SocketAddr,
    broadcast: bool,
    reuse_address: bool,
    reuse_port: bool,
}

impl UdpConnectConfig {
    /// Specify the source address and port.
    pub fn with_source_address(mut self, addr: SocketAddr) -> Self {
        self.source_address = addr;
        self
    }

    /// Enables the socket capabilities to send broadcast messages.
    pub fn with_broadcast(mut self) -> Self {
        self.broadcast = true;
        self
    }

    /// Set value for the `SO_REUSEADDR` option on this socket. This indicates that futher calls to
    /// `bind` may allow reuse of local addresses. For IPv4 sockets this means that a socket may
    /// bind even when there’s a socket already listening on this port.
    pub fn with_reuse_address(mut self) -> Self {
        self.reuse_address = true;
        self
    }

    /// Set value for the `SO_REUSEPORT` option on this socket. This indicates that further calls
    /// to `bind` may allow reuse of local addresses. For IPv4 sockets this means that a socket may
    /// bind even when there’s a socket already listening on this port. This option is always-on on
    /// Windows and cannot be configured.
    pub fn with_reuse_port(mut self) -> Self {
        self.reuse_port = true;
        self
    }
}

impl Default for UdpConnectConfig {
    fn default() -> Self {
        Self {
            source_address: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into(),
            broadcast: false,
            reuse_address: false,
            reuse_port: false,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct UdpListenConfig {
    send_broadcasts: bool,
    receive_broadcasts: bool,
    reuse_address: bool,
    reuse_port: bool,
}

impl UdpListenConfig {
    /// Enables the socket capabilities to send broadcast messages when the listening socket is
    /// also used for sending with
    /// [`Endpoint::from_listener`](crate::network::Endpoint::from_listener).
    pub fn with_send_broadcasts(mut self) -> Self {
        self.send_broadcasts = true;
        self
    }

    /// On Windows, when listening on a specific IP address, the sockets also receives
    /// corresponding subnet broadcasts and global broadcasts ([`std::net::Ipv4Addr::BROADCAST`])
    /// received on the interface matching the IP.  When this option is set, message-io mimics this
    /// behavior on Linux.
    pub fn with_receive_broadcasts(mut self) -> Self {
        self.receive_broadcasts = true;
        self
    }

    /// Set value for the `SO_REUSEADDR` option on this socket. This indicates that futher calls to
    /// `bind` may allow reuse of local addresses.
    pub fn with_reuse_address(mut self) -> Self {
        self.reuse_address = true;
        self
    }

    /// Set value for the `SO_REUSEPORT` option on this socket. This indicates that further calls
    /// to `bind` may allow reuse of local addresses. For IPv4 sockets this means that a socket may
    /// bind even when there’s a socket already listening on this port. This option is always-on
    /// on Windows and cannot be configured.
    pub fn with_reuse_port(mut self) -> Self {
        self.reuse_port = true;
        self
    }
}

pub(crate) struct UdpAdapter;
impl Adapter for UdpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub(crate) struct RemoteResource {
    socket: UdpSocket,
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.socket
    }
}

impl Remote for RemoteResource {
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        let config = match config {
            TransportConnect::Udp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        let peer_addr = *remote_addr.socket_addr();

        let socket = Socket::new(
            match peer_addr {
                SocketAddr::V4 { .. } => Domain::IPV4,
                SocketAddr::V6 { .. } => Domain::IPV6,
            },
            Type::DGRAM,
            Some(Protocol::UDP),
        )?;
        socket.set_nonblocking(true)?;

        socket.set_reuse_address(config.reuse_address)?;
        #[cfg(unix)]
        socket.set_reuse_port(config.reuse_port)?;
        socket.set_broadcast(config.broadcast)?;

        socket.bind(&config.source_address.into())?;
        socket.connect(&peer_addr.into())?;

        let socket = UdpSocket::from_std(socket.into());
        let local_addr = socket.local_addr()?;
        Ok(ConnectionInfo { remote: RemoteResource { socket }, local_addr, peer_addr })
    }

    fn receive(&self, mut process_data: impl FnMut(&[u8])) -> ReadStatus {
        let buffer: MaybeUninit<[u8; MAX_LOCAL_PAYLOAD_LEN]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.socket.recv(&mut input_buffer) {
                Ok(size) => process_data(&mut input_buffer[..size]),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                    break ReadStatus::WaitNextEvent
                }
                Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                    // Avoid ICMP generated error to be logged
                    break ReadStatus::WaitNextEvent
                }
                Err(err) => {
                    log::error!("UDP receive error: {}", err);
                    break ReadStatus::WaitNextEvent // Should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        send_packet(data, |data| self.socket.send(data))
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        PendingStatus::Ready
    }
}

pub(crate) struct LocalResource {
    socket: UdpSocket,
    #[cfg(target_os = "linux")]
    ingress_addresses: Option<Vec<IpAddr>>,
}

impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.socket
    }
}

#[cfg(target_os = "linux")]
impl LocalResource {
    fn accept_filtered(
        &self,
        ingress_addresses: &[IpAddr],
        mut accept_remote: impl FnMut(AcceptedType<'_, RemoteResource>),
    ) {
        let buffer: MaybeUninit<[u8; MAX_LOCAL_PAYLOAD_LEN]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array
        let mut control_buffer = nix::cmsg_space!(libc::sockaddr_storage);

        loop {
            let mut iov = [io::IoSliceMut::new(&mut input_buffer)];
            let result = socket::recvmsg::<SockaddrStorage>(
                self.socket.as_raw_fd(),
                &mut iov,
                Some(&mut control_buffer),
                MsgFlags::empty(),
            );

            match result {
                Ok(msg) => {
                    let size = msg.bytes;

                    let ingress_ip = match msg.cmsgs().find_map(|cmsg| match cmsg {
                        ControlMessageOwned::Ipv4PacketInfo(pktinfo) => {
                            Some(Ipv4Addr::from(pktinfo.ipi_addr.s_addr.to_be()).into())
                        }
                        ControlMessageOwned::Ipv6PacketInfo(pktinfo) => {
                            Some(Ipv6Addr::from(pktinfo.ipi6_addr.s6_addr).into())
                        }
                        _ => None,
                    }) {
                        Some(ingress_ip) => ingress_ip,
                        None => continue,
                    };

                    if !ingress_addresses.contains(&ingress_ip) {
                        continue
                    }

                    fn convert_sockaddr(addr: SockaddrStorage) -> Option<SocketAddr> {
                        if let Some(addr) = addr.as_sockaddr_in() {
                            return Some(SocketAddr::V4((*addr).into()))
                        }
                        if let Some(addr) = addr.as_sockaddr_in6() {
                            return Some(SocketAddr::V6((*addr).into()))
                        }
                        None
                    }

                    let addr = match msg.address.and_then(convert_sockaddr) {
                        Some(addr) => addr,
                        None => continue,
                    };

                    let data = &mut input_buffer[..size];
                    accept_remote(AcceptedType::Data(addr, data))
                }
                Err(Errno::EWOULDBLOCK) => break,
                Err(err) => break log::error!("UDP accept error: {}", err), // Should never happen
            }
        }
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen_with(
        config: TransportListen,
        #[cfg(not(target_os = "linux"))] addr: SocketAddr,
        #[cfg(target_os = "linux")] mut addr: SocketAddr,
    ) -> io::Result<ListeningInfo<Self>> {
        let config = match config {
            TransportListen::Udp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        let multicast = match addr {
            SocketAddr::V4(addr) if addr.ip().is_multicast() => Some(addr),
            _ => None,
        };

        let socket = Socket::new(
            match addr {
                SocketAddr::V4 { .. } => Domain::IPV4,
                SocketAddr::V6 { .. } => Domain::IPV6,
            },
            Type::DGRAM,
            Some(Protocol::UDP),
        )?;
        socket.set_nonblocking(true)?;

        if config.reuse_address || multicast.is_some() {
            socket.set_reuse_address(true)?;
        }
        #[cfg(unix)]
        if config.reuse_port || multicast.is_some() {
            socket.set_reuse_port(true)?;
        }
        socket.set_broadcast(config.send_broadcasts)?;

        #[cfg(target_os = "linux")]
        let ingress_addresses = if config.receive_broadcasts {
            // enable the socket packet info option
            match addr {
                SocketAddr::V4 { .. } => {
                    socket::setsockopt(socket.as_raw_fd(), sockopt::Ipv4PacketInfo, &true)?
                }
                SocketAddr::V6 { .. } => {
                    socket::setsockopt(socket.as_raw_fd(), sockopt::Ipv6RecvPacketInfo, &true)?
                }
            }

            // find ifaddr matching the listen addr
            let ifaddr = getifaddrs()?.find_map(|ifaddr| {
                ifaddr.address.and_then(|ss| {
                    match (
                        ss.as_sockaddr_in().map(|si| Ipv4Addr::from(si.ip())),
                        ss.as_sockaddr_in6().map(|si| si.ip()),
                    ) {
                        (Some(ip4), _) if IpAddr::V4(ip4) == addr.ip() => Some(ifaddr),
                        (_, Some(ip6)) if IpAddr::V6(ip6) == addr.ip() => Some(ifaddr),
                        _ => None,
                    }
                })
            });

            match ifaddr {
                None => return Err(ErrorKind::AddrNotAvailable.into()),
                Some(ifaddr) => {
                    // Get allowed ingress IP addresses
                    let mut ingress_addresses = vec![addr.ip()];

                    // Some interfaces like VPN adapters don't have broadcast support.
                    if let Some(broadcast_ss) = ifaddr.broadcast {
                        if let Some(si) = broadcast_ss.as_sockaddr_in() {
                            ingress_addresses.push(Ipv4Addr::from(si.ip()).into());
                            ingress_addresses.push(Ipv4Addr::BROADCAST.into());
                        }
                        if let Some(si) = broadcast_ss.as_sockaddr_in6() {
                            ingress_addresses.push(si.ip().into());
                        }
                    }

                    // Bind the socket to the specific interface
                    socket::setsockopt(
                        socket.as_raw_fd(),
                        sockopt::BindToDevice,
                        &ifaddr.interface_name.into(),
                    )?;

                    // Listen on UNSPECIFIED
                    addr.set_ip(match addr {
                        SocketAddr::V4 { .. } => Ipv4Addr::UNSPECIFIED.into(),
                        SocketAddr::V6 { .. } => Ipv6Addr::UNSPECIFIED.into(),
                    });

                    Some(ingress_addresses)
                }
            }
        }
        else {
            None
        };

        if let Some(multicast) = multicast {
            socket.join_multicast_v4(multicast.ip(), &Ipv4Addr::UNSPECIFIED)?;
            socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port()).into())?;
        }
        else {
            socket.bind(&addr.into())?;
        }

        let socket = UdpSocket::from_std(socket.into());
        let local_addr = socket.local_addr().unwrap();
        Ok(ListeningInfo {
            local: {
                LocalResource {
                    socket,
                    #[cfg(target_os = "linux")]
                    ingress_addresses,
                }
            },
            local_addr,
        })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        #[cfg(target_os = "linux")]
        if let Some(ingress_addresses) = &self.ingress_addresses {
            self.accept_filtered(ingress_addresses, accept_remote);
            return
        }

        let buffer: MaybeUninit<[u8; MAX_LOCAL_PAYLOAD_LEN]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.socket.recv_from(&mut input_buffer) {
                Ok((size, addr)) => {
                    let data = &mut input_buffer[..size];
                    accept_remote(AcceptedType::Data(addr, data))
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => break log::error!("UDP accept error: {}", err), // Should never happen
            };
        }
    }

    fn send_to(&self, addr: SocketAddr, data: &[u8]) -> SendStatus {
        send_packet(data, |data| self.socket.send_to(data, addr))
    }
}

impl Drop for LocalResource {
    fn drop(&mut self) {
        if let SocketAddr::V4(addr) = self.socket.local_addr().unwrap() {
            if addr.ip().is_multicast() {
                self.socket.leave_multicast_v4(addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
            }
        }
    }
}

fn send_packet(data: &[u8], send_method: impl Fn(&[u8]) -> io::Result<usize>) -> SendStatus {
    loop {
        match send_method(data) {
            Ok(_) => break SendStatus::Sent,
            // Avoid ICMP generated error to be logged
            Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                break SendStatus::ResourceNotFound
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => continue,
            Err(ref err) if err.kind() == ErrorKind::Other => {
                break SendStatus::MaxPacketSizeExceeded
            }
            Err(err) => {
                log::error!("UDP send error: {}", err);
                break SendStatus::ResourceNotFound // should not happen
            }
        }
    }
}
