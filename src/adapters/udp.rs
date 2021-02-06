use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::adapter::{AdapterEvent, Adapter, Controller, Processor};
use crate::poll::{PollRegister};
use crate::util::{OTHER_THREAD_ERR, SendingStatus};

use mio::net::{UdpSocket};

use net2::{UdpBuilder};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock};
use std::io::{self, ErrorKind};

/// Maximun payload that a UDP packet can hold:
/// - 9216: MTU of the OS with the minimun MTU: OSX
/// - 20: max IP header
/// - 8: max udp header
/// The serialization of your message must not exceed this value.
pub const MAX_UDP_LEN: usize = 9216 - 20 - 8;

struct Store {
    // We store the addr because we will need it when the stream crash.
    // When a stream crash by an error (i.e. reset) peer_addr no longer returns the addr.
    sockets: RwLock<HashMap<ResourceId, (UdpSocket, SocketAddr)>>,
    listeners: RwLock<HashMap<ResourceId, UdpSocket>>,
}

impl Store {
    fn new() -> Self {
        Self { sockets: RwLock::new(HashMap::new()), listeners: RwLock::new(HashMap::new()) }
    }
}

pub struct UdpAdapter;

impl<C> Adapter<C> for UdpAdapter
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    type Controller = UdpController;
    type Processor = UdpProcessor;
    fn split(self, poll_register: PollRegister) -> (UdpController, UdpProcessor) {
        let store = Arc::new(Store::new());
        (UdpController::new(store.clone(), poll_register), UdpProcessor::new(store))
    }
}

pub struct UdpController {
    store: Arc<Store>,
    poll_register: PollRegister,
}

impl UdpController {
    fn new(store: Arc<Store>, poll_register: PollRegister) -> Self {
        Self { store, poll_register }
    }
}

impl Controller for UdpController {
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint> {
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        socket.connect(addr)?;

        let id = self.poll_register.add(&mut socket, ResourceType::Remote);
        self.store.sockets.write().expect(OTHER_THREAD_ERR).insert(id, (socket, addr));
        Ok(Endpoint::new(id, addr))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let mut socket = match addr {
            SocketAddr::V4(addr) if addr.ip().is_multicast() => {
                let listening_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, addr.port());
                let socket = UdpBuilder::new_v4()?.reuse_address(true)?.bind(listening_addr)?;
                socket.set_nonblocking(true)?;
                socket.join_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED)?;
                UdpSocket::from_std(socket)
            }
            _ => UdpSocket::bind(addr)?,
        };

        let id = self.poll_register.add(&mut socket, ResourceType::Listener);
        let real_addr = socket.local_addr().unwrap();
        self.store.listeners.write().expect(OTHER_THREAD_ERR).insert(id, socket);
        Ok((id, real_addr))
    }

    fn remove(&mut self, id: ResourceId) -> Option<()> {
        let poll_register = &mut self.poll_register;
        match id.resource_type() {
            ResourceType::Listener => self
                .store
                .listeners
                .write()
                .expect(OTHER_THREAD_ERR)
                .remove(&id)
                .map(|mut listener| poll_register.remove(&mut listener)),
            ResourceType::Remote => self
                .store
                .sockets
                .write()
                .expect(OTHER_THREAD_ERR)
                .remove(&id)
                .map(|(mut socket, _)| poll_register.remove(&mut socket)),
        }
    }

    fn local_addr(&self, id: ResourceId) -> Option<SocketAddr> {
        match id.resource_type() {
            ResourceType::Listener => self
                .store
                .listeners
                .read()
                .expect(OTHER_THREAD_ERR)
                .get(&id)
                .map(|listener| listener.local_addr().unwrap()),
            ResourceType::Remote => self
                .store
                .sockets
                .read()
                .expect(OTHER_THREAD_ERR)
                .get(&id)
                .map(|(socket, _)| socket.local_addr().unwrap()),
        }
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus {
        if data.len() > MAX_UDP_LEN {
            log::error!(
                "The UDP message could not set because it exceeds the MTU. \
                Current size: {}, MTU: {}",
                data.len(),
                MAX_UDP_LEN
            );
            SendingStatus::MaxPacketSizeExceeded(data.len(), MAX_UDP_LEN)
        }
        else if let Some((socket, _)) =
            self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&endpoint.resource_id())
        {
            socket.send(data).expect("Errors already managed");
            SendingStatus::Sent
        }
        else if let Some(socket) =
            self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&endpoint.resource_id())
        {
            socket.send_to(data, endpoint.addr()).expect("Errors already managed");
            SendingStatus::Sent
        }
        else {
            // We can safety panics here because it is a programming error by the user.
            panic!("Error: you are sending over an already removed endpoint");
        }
    }
}

impl Drop for UdpController {
    fn drop(&mut self) {
        for socket in self.store.listeners.write().expect(OTHER_THREAD_ERR).values_mut() {
            if let SocketAddr::V4(addr) = socket.local_addr().unwrap() {
                if addr.ip().is_multicast() {
                    socket.leave_multicast_v4(&addr.ip(), &Ipv4Addr::UNSPECIFIED).unwrap();
                }
            }
        }
    }
}

pub struct UdpProcessor {
    store: Arc<Store>,
    input_buffer: [u8; MAX_UDP_LEN],
}

impl UdpProcessor {
    fn new(store: Arc<Store>) -> Self {
        Self { store, input_buffer: [0; MAX_UDP_LEN] }
    }
}

impl<C> Processor<C> for UdpProcessor
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    fn process_listener(&mut self, id: ResourceId, event_callback: &mut C) {
        if let Some(socket) = self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id) {
            loop {
                match socket.recv_from(&mut self.input_buffer) {
                    Ok((size, addr)) => {
                        let endpoint = Endpoint::new(id, addr);
                        let data = &mut self.input_buffer[..size];
                        event_callback(endpoint, AdapterEvent::Data(data));
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        log::error!("UDP process listener error");
                        break // should not happen
                    }
                }
            }
        }
    }

    fn process_remote(&mut self, id: ResourceId, event_callback: &mut C) {
        if let Some((socket, addr)) = self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            loop {
                match socket.recv(&mut self.input_buffer) {
                    Ok(size) => {
                        let data = &mut self.input_buffer[..size];
                        event_callback(endpoint, AdapterEvent::Data(data));
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        log::error!("UDP process remote error");
                        break // should not happen
                    }
                }
            }
        }
    }
}
