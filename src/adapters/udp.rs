use crate::adapter::{
    Resource, Adapter, ActionHandler, EventHandler, SendStatus, AcceptedType, ReadStatus,
};

use mio::net::{UdpSocket};
use mio::event::{Source};

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

pub struct ClientResource(UdpSocket);
impl Resource for ClientResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.0
    }
}

pub struct ServerResource(UdpSocket);
impl Resource for ServerResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.0
    }
}

impl Drop for ServerResource {
    fn drop(&mut self) {
        if let SocketAddr::V4(addr) = self.0.local_addr().unwrap() {
            if addr.ip().is_multicast() {
                self.0.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
            }
        }
    }
}

pub struct UdpAdapter;
impl Adapter for UdpAdapter {
    type Remote = ClientResource;
    type Listener = ServerResource;
    type ActionHandler = UdpActionHandler;
    type EventHandler = UdpEventHandler;

    fn split(self) -> (UdpActionHandler, UdpEventHandler) {
        (UdpActionHandler, UdpEventHandler::default())
    }
}

pub struct UdpActionHandler;
impl ActionHandler for UdpActionHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<ClientResource> {
        let socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        socket.connect(addr)?;
        Ok(ClientResource(socket))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ServerResource, SocketAddr)> {
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
        Ok((ServerResource(socket), real_addr))
    }

    fn send(&mut self, socket: &ClientResource, data: &[u8]) -> SendStatus {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            Self::udp_length_exceeded(data.len())
        }
        else {
            Self::sending_status(socket.0.send(data))
        }
    }

    fn send_by_listener(
        &mut self,
        socket: &ServerResource,
        addr: SocketAddr,
        data: &[u8],
    ) -> SendStatus
    {
        if data.len() > MAX_UDP_PAYLOAD_LEN {
            Self::udp_length_exceeded(data.len())
        }
        else {
            Self::sending_status(socket.0.send_to(data, addr))
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
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn accept_event(
        &mut self,
        socket: &ServerResource,
        accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>),
    )
    {
        loop {
            match socket.0.recv_from(&mut self.input_buffer) {
                Ok((size, addr)) => {
                    let data = &mut self.input_buffer[..size];
                    accept_remote(AcceptedType::Data(addr, data))
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break log::trace!("UDP accept event error"), // Should never happen
            };
        }
    }

    fn read_event(&mut self, socket: &ClientResource, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        loop {
            match socket.0.recv(&mut self.input_buffer) {
                Ok(size) => process_data(&mut self.input_buffer[..size]),
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
}
