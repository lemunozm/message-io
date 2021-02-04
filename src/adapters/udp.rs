use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::mio_engine::{MioRegister, AdapterEvent};
use crate::util::{OTHER_THREAD_ERR, SendingStatus};

use mio::net::{UdpSocket};

use net2::{UdpBuilder};

use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock};
use std::io::{self, ErrorKind};

pub const MAX_UDP_LEN: usize = 1488;

struct Store {
    // We store the addr because we will need it when the stream crash.
    // When a stream crash by an error (i.e. reset) peer_addr no longer returns the addr.
    sockets: RwLock<HashMap<ResourceId, (UdpSocket, SocketAddr)>>,
    listeners: RwLock<HashMap<ResourceId, UdpSocket>>,
}

impl Store {
    fn new() -> Self {
        Self {
            sockets: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
        }
    }
}

pub struct UdpAdapter;

impl UdpAdapter {
    pub fn split(mio_register: MioRegister) -> (UdpController, UdpProcessor) {
        let store = Arc::new(Store::new());
        (
            UdpController::new(store.clone(), mio_register),
            UdpProcessor::new(store),
        )
    }
}

pub struct UdpController {
    store: Arc<Store>,
    mio_register: MioRegister,
}

impl UdpController {
    fn new(store: Arc<Store>, mio_register: MioRegister) -> Self {
        Self {store, mio_register}
    }

    pub fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint> {
        let mut socket = UdpSocket::bind("0.0.0.0:0".parse().unwrap())?;
        socket.connect(addr)?;

        let id = self.mio_register.add(&mut socket, ResourceType::Remote);
        self.store.sockets.write().expect(OTHER_THREAD_ERR).insert(id, (socket, addr));
        Ok(Endpoint::new(id, addr))
    }

    pub fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
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

        let id = self.mio_register.add(&mut socket, ResourceType::Listener);
        let real_addr = socket.local_addr().unwrap();
        self.store.listeners.write().expect(OTHER_THREAD_ERR).insert(id, socket);
        Ok((id, real_addr))
    }

    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        let mio_register = &mut self.mio_register;
        match id.resource_type() {
            ResourceType::Listener => {
                self.store.listeners.write().expect(OTHER_THREAD_ERR).remove(&id).map(
                    |mut listener| mio_register.remove(&mut listener)
                )
            }
            ResourceType::Remote => {
                self.store.sockets.write().expect(OTHER_THREAD_ERR).remove(&id).map(
                    |(mut socket, _)| mio_register.remove(&mut socket)
                )
            }
        }
    }

    pub fn local_address(&self, id: ResourceId) -> Option<SocketAddr> {
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

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus {
        if data.len() > MAX_UDP_LEN {
            SendingStatus::MaxPacketSizeExceeded(data.len(), MAX_UDP_LEN)
        }
        else {
            // Only two errors can happend in 'sends' methods of UDP:
            // A packet that exceeds MTU size and send() called without knowing the remote addr.
            // Both controlled so we can expect that no errors be produced at sending

            if let Some((socket, _)) =
                self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&endpoint.resource_id())
            {
                socket.send(data).expect("No errors here");
                SendingStatus::Sent
            }
            else if let Some(socket) =
                self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&endpoint.resource_id())
            {
                socket.send_to(data, endpoint.addr()).expect("No errors here");
                SendingStatus::Sent
            }
            else {
                // If the endpoint does not exists, is because it was removed
                SendingStatus::RemovedEndpoint
            }
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

    pub fn process<C>(&mut self, id: ResourceId, event_callback: C)
    where C: FnMut(Endpoint, AdapterEvent<'_>) {
        match id.resource_type() {
            ResourceType::Listener => {
                self.process_listener(id, event_callback)
            }
            ResourceType::Remote => {
                self.process_remote(id, event_callback)
            }
        }
    }

    fn process_listener<C>(&mut self, id: ResourceId, mut event_callback: C)
    where C: FnMut(Endpoint, AdapterEvent<'_>) {
        // could have been produced before removing it.
        if let Some(socket) = self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id) {
            loop {
                match socket.recv_from(&mut self.input_buffer) {
                    Ok((size, addr)) => {
                        let endpoint = Endpoint::new(id, addr);
                        let data = &mut self.input_buffer[..size];
                        event_callback(endpoint, AdapterEvent::Data(data));
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => break, // should not happened
                }
            }
        }
    }

    fn process_remote<C>(&mut self, id: ResourceId, mut event_callback: C)
    where C: FnMut(Endpoint, AdapterEvent<'_>) {
        if let Some((socket, addr)) = self.store.sockets.read().expect(OTHER_THREAD_ERR).get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            loop {
                match socket.recv(&mut self.input_buffer) {
                    Ok(size) => {
                        let data = &mut self.input_buffer[..size];
                        event_callback(endpoint, AdapterEvent::Data(data));
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => continue,
                    Err(_) => break, // should not happened
                }
            }
        }
    }
}
