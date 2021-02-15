use crate::adapter::{Resource, Remote, Listener, Adapter, SendStatus, AcceptedType, ReadStatus};
use crate::remote_addr::{RemoteAddr};

use mio::net::{UdpSocket};
use mio::event::{Source};

use net2::{UdpBuilder};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::io::{self, ErrorKind};
use std::mem::{MaybeUninit};

/// Maximun payload that a UDP packet can send safety in main OS.
/// - 9216: MTU of the OS with the minimun MTU: OSX
/// - 20: max IP header
/// - 8: max udp header
/// The serialization of your message must not exceed this value.
pub const MAX_UDP_PAYLOAD_LEN: usize = 9216 - 20 - 8;

// The reception buffer can reach the UDP standard size.
const INPUT_BUFFER_SIZE: usize = 65535 - 20 - 8;

pub struct UdpAdapter;
impl Adapter for UdpAdapter {
    type Remote = RemoteResource;
    type Listener = ListenerResource;
}

pub struct RemoteResource {
    socket: UdpSocket,
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.socket
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<(Self, SocketAddr)> {
        let socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        let addr = remote_addr.socket_addr();
        socket.connect(*addr)?;
        Ok((RemoteResource { socket }, *addr))
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        let buffer: MaybeUninit<[u8; INPUT_BUFFER_SIZE]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.socket.recv(&mut input_buffer) {
                Ok(size) => process_data(&mut input_buffer[..size]),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                    break ReadStatus::WaitNextEvent
                }
                Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                    // Avoid ICMP generated error to be logged
                    break ReadStatus::Disconnected
                }
                Err(_) => {
                    log::error!("UDP read event error");
                    break ReadStatus::Disconnected // Should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            udp_length_exceeded(data.len())
        }
        else {
            to_send_status(self.socket.send(data))
        }
    }
}

pub struct ListenerResource {
    socket: UdpSocket,
}

impl Resource for ListenerResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.socket
    }
}

impl Listener for ListenerResource {
    type Remote = RemoteResource;

    fn listen(addr: SocketAddr) -> io::Result<(Self, SocketAddr)> {
        let socket = match addr {
            SocketAddr::V4(addr) if addr.ip().is_multicast() => {
                let listening_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port());
                let socket = UdpBuilder::new_v4()?.reuse_address(true)?.bind(listening_addr)?;
                socket.set_nonblocking(true)?;
                socket.join_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
                UdpSocket::from_std(socket)
            }
            _ => UdpSocket::bind(addr)?,
        };

        let real_addr = socket.local_addr().unwrap();
        Ok((ListenerResource { socket }, real_addr))
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        let buffer: MaybeUninit<[u8; INPUT_BUFFER_SIZE]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.socket.recv_from(&mut input_buffer) {
                Ok((size, addr)) => {
                    let data = &mut input_buffer[..size];
                    accept_remote(AcceptedType::Data(addr, data))
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break log::trace!("UDP accept event error"), // Should never happen
            };
        }
    }

    fn send_to(&self, addr: SocketAddr, data: &[u8]) -> SendStatus {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            udp_length_exceeded(data.len())
        }
        else {
            to_send_status(self.socket.send_to(data, addr))
        }
    }
}

impl Drop for ListenerResource {
    fn drop(&mut self) {
        if let SocketAddr::V4(addr) = self.socket.local_addr().unwrap() {
            if addr.ip().is_multicast() {
                self.socket.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
            }
        }
    }
}

fn udp_length_exceeded(length: usize) -> SendStatus {
    log::error!(
        "The UDP message could not be sent because it exceeds the MTU. \
        Current size: {}, MTU: {}",
        length,
        MAX_UDP_PAYLOAD_LEN
    );
    SendStatus::MaxPacketSizeExceeded(length, MAX_UDP_PAYLOAD_LEN)
}

fn to_send_status(result: io::Result<usize>) -> SendStatus {
    match result {
        Ok(_) => SendStatus::Sent,
        // Avoid ICMP generated error to be logged
        Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => SendStatus::ResourceNotFound,
        Err(_) => {
            log::error!("UDP send remote error");
            SendStatus::ResourceNotFound
        }
    }
}
