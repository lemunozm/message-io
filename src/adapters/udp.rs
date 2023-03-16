use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};

use mio::net::{UdpSocket};
use mio::event::{Source};

use socket2::{Socket, Domain, Type, Protocol};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::io::{self, ErrorKind};
use std::mem::{MaybeUninit};

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
        _: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        let socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        let peer_addr = *remote_addr.socket_addr();
        socket.connect(peer_addr)?;
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
}

impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.socket
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen_with(_: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let socket = match addr {
            SocketAddr::V4(addr) if addr.ip().is_multicast() => {
                let listening_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port());

                let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
                socket.set_reuse_address(true)?;
                #[cfg(unix)]
                socket.set_reuse_port(true)?;
                socket.set_nonblocking(true)?;
                socket.join_multicast_v4(addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
                socket.bind(&listening_addr.into())?;
                UdpSocket::from_std(socket.into())
            }
            _ => UdpSocket::bind(addr)?,
        };

        let local_addr = socket.local_addr().unwrap();
        Ok(ListeningInfo { local: { LocalResource { socket } }, local_addr })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
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
