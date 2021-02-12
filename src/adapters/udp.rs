use crate::adapter::{Adapter, ActionHandler, EventHandler};
use crate::status::{SendStatus, AcceptStatus, ReadStatus};

use mio::net::{UdpSocket};

use net2::{UdpBuilder};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::io::{self, ErrorKind};

/// Maximun payload that a UDP packet can send safety in main OS.
/// - 9216: MTU of the OS with the minimun MTU: OSX
/// - 20: max IP header
/// - 8: max udp header
/// The serialization of your message must not exceed this value.
pub const MAX_UDP_PAYLOAD_LEN: usize = 9216 - 20 - 8;

// The reception buffer reach the UDP standard size.
const MAX_UDP_PAYLOAD_BUFFER_LEN: usize = 65535 - 20 - 8;

pub struct UdpAdapter;

impl Adapter for UdpAdapter {
    type Remote = UdpSocket;
    type Listener = UdpSocket;
    type ActionHandler = UdpActionHandler;
    type EventHandler = UdpEventHandler;

    fn split(self) -> (UdpActionHandler, UdpEventHandler) {
        (UdpActionHandler, UdpEventHandler::default())
    }
}

pub struct UdpActionHandler;
impl ActionHandler for UdpActionHandler {
    type Remote = UdpSocket;
    type Listener = UdpSocket;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        socket.connect(addr)?;
        Ok(socket)
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(UdpSocket, SocketAddr)> {
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
        Ok((socket, real_addr))
    }

    fn remove_listener(&mut self, socket: UdpSocket, local_addr: SocketAddr) {
        if let SocketAddr::V4(addr) = local_addr {
            if addr.ip().is_multicast() {
                socket.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
            }
        }
    }

    fn send(&mut self, socket: &UdpSocket, data: &[u8]) -> SendStatus {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            Self::udp_length_exceeded(data.len())
        }
        else {
            Self::sending_status(socket.send(data))
        }
    }

    fn send_by_listener(
        &mut self,
        socket: &UdpSocket,
        addr: SocketAddr,
        data: &[u8],
    ) -> SendStatus
    {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            Self::udp_length_exceeded(data.len())
        }
        else {
            Self::sending_status(socket.send_to(data, addr))
        }
    }
}

impl UdpActionHandler {
    fn udp_length_exceeded(length: usize) -> SendStatus {
        log::error!(
            "The UDP message could not be sent because it exceeds the MTU. \
            Current size: {}, MTU: {}",
            length,
            MAX_UDP_PAYLOAD_LEN
        );
        SendStatus::MaxPacketSizeExceeded(length, MAX_UDP_PAYLOAD_LEN)
    }

    fn sending_status(result: io::Result<usize>) -> SendStatus {
        match result {
            Ok(_) => SendStatus::Sent,
            // Avoid ICMP generated error to be logged
            Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                SendStatus::ResourceNotFound
            }
            Err(_) => {
                log::error!("UDP send remote error");
                SendStatus::ResourceNotFound
            }
        }
    }
}

pub struct UdpEventHandler {
    input_buffer: [u8; MAX_UDP_PAYLOAD_BUFFER_LEN],
}

impl Default for UdpEventHandler {
    fn default() -> Self {
        Self { input_buffer: [0; MAX_UDP_PAYLOAD_BUFFER_LEN] }
    }
}

impl EventHandler for UdpEventHandler {
    type Remote = UdpSocket;
    type Listener = UdpSocket;

    fn accept_event(&mut self, socket: &UdpSocket) -> AcceptStatus<'_, Self::Remote> {
        match socket.recv_from(&mut self.input_buffer) {
            Ok((size, addr)) => {
                let data = &mut self.input_buffer[..size];
                AcceptStatus::AcceptedData(addr, data)
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => AcceptStatus::WaitNextEvent,
            Err(_) => {
                log::trace!("UDP accept event error");
                AcceptStatus::WaitNextEvent // Should not happen
            }
        }
    }

    fn read_event(&mut self, socket: &UdpSocket, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        match socket.recv(&mut self.input_buffer) {
            Ok(size) => {
                process_data(&mut self.input_buffer[..size]);
                ReadStatus::WaitNextEvent // recv gives only one datagram
            }
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => ReadStatus::WaitNextEvent,
            // Avoid ICMP generated error to be logged
            Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => ReadStatus::Disconnected,
            Err(_) => {
                log::error!("UDP read event error");
                ReadStatus::Disconnected // Should not happen
            }
        }
    }
}
