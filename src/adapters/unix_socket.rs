#![allow(unused_variables)]

use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};

use mio::event::{Source};
use mio::net::{UnixDatagram, UnixListener, UnixStream};

use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::fs;

// Note: net.core.rmem_max = 212992 by default on linux systems
// not used because w euse unixstream I think?
// TODO: delete this if I PR
pub const MAX_PAYLOAD_LEN: usize = 212992;

/// From tcp.rs
/// Size of the internal reading buffer.
/// It implies that at most the generated [`crate::network::NetEvent::Message`]
/// will contains a chunk of data of this value.
pub const INPUT_BUFFER_SIZE: usize = u16::MAX as usize; // 2^16 - 1

// We don't use the SocketAddr, we just striaght up get the path from config.
pub fn create_null_socketaddr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0,0,0,0)), 0)
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UnixSocketListenConfig {
    path: PathBuf,
}

impl UnixSocketListenConfig {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Default for UnixSocketListenConfig {
    fn default() -> Self {
        // TODO: better idea? I could make this into an option later and complain if empty.
        Self { path: "/tmp/mio.sock".into() }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct UnixSocketConnectConfig {
    path: PathBuf,
}

impl UnixSocketConnectConfig {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}


impl Default for UnixSocketConnectConfig {
    fn default() -> Self {
        // TODO: better idea? I could make this into an option later and complain if empty.
        Self { path: "/tmp/mio.sock".into() }
    }
}

pub(crate) struct UnixSocketStreamAdapter;
impl Adapter for UnixSocketStreamAdapter {
    type Remote = StreamRemoteResource;
    type Local = StreamLocalResource;
}

pub(crate) struct StreamRemoteResource {
    stream: UnixStream
}

impl Resource for StreamRemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.stream
    }
}

// taken from tcp impl
pub fn check_stream_ready(stream: &UnixStream) -> PendingStatus{
    if let Ok(Some(_)) = stream.take_error() {
        return PendingStatus::Disconnected;
    } 

    return PendingStatus::Ready;
}

impl Remote for StreamRemoteResource {
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {

        let stream_config = match config {
            TransportConnect::UnixSocketStream(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };
        
        match UnixStream::connect(stream_config.path) {
            Ok(stream) => {
                Ok(ConnectionInfo {
                    remote: Self {
                        stream
                    },
                    // the unixstream uses SocketAddr from mio that can't be converted
                    local_addr: create_null_socketaddr(), // stream.local_addr()?,
                    peer_addr: create_null_socketaddr() // stream.peer_addr()?.into(),
                })
            },
            Err(err) => {
                return Err(err);
            },
        }

        
    }

    fn receive(&self, mut process_data: impl FnMut(&[u8])) -> ReadStatus {
        // Most of this is reused from tcp.rs
        let buffer: MaybeUninit<[u8; INPUT_BUFFER_SIZE]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            let stream = &self.stream;
            match stream.deref().read(&mut input_buffer) {
                Ok(0) => break ReadStatus::Disconnected,
                Ok(size) => process_data(&input_buffer[..size]),
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                    break ReadStatus::WaitNextEvent
                }
                Err(ref err) if err.kind() == ErrorKind::ConnectionReset => {
                    break ReadStatus::Disconnected
                }
                Err(err) => {
                    log::error!("Unix socket receive error: {}", err);
                    break ReadStatus::Disconnected // should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        // Most of this is reused from tcp.rs
        let mut total_bytes_sent = 0;
        loop {
            let stream = &self.stream;
            match stream.deref().write(&data[total_bytes_sent..]) {
                Ok(bytes_sent) => {
                    total_bytes_sent += bytes_sent;
                    if total_bytes_sent == data.len() {
                        break SendStatus::Sent
                    }
                }
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,

                // Others errors are considered fatal for the connection.
                // a Event::Disconnection will be generated later.
                Err(err) => {
                    log::error!("unix socket receive error: {}", err);
                    break SendStatus::ResourceNotFound // should not happen
                }
            }
        }
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        check_stream_ready(&self.stream)
    }
}

pub(crate) struct StreamLocalResource {
    listener: UnixListener,
    bind_path: PathBuf
}

impl Resource for StreamLocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.listener
    }
}

impl Drop for StreamLocalResource {
    fn drop(&mut self) {
        // this may fail if the file is already removed
        match fs::remove_file(&self.bind_path) {
            Ok(_) => (),
            Err(err) => log::error!("Error removing unix socket file on drop: {}", err),
        }
    }
}

impl Local for StreamLocalResource {
    type Remote = StreamRemoteResource;

    fn listen_with(config: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let config = match config {
            TransportListen::UnixStreamSocket(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        // TODO: fallback to ip when we are able to set path to none
        let listener = UnixListener::bind(&config.path)?;
        let local_addr = listener.local_addr()?;
        Ok(ListeningInfo {
            local: Self {
                listener,
                bind_path: config.path
            },
            // same issue as above my change in https://github.com/tokio-rs/mio/pull/1749
            // relevant issue https://github.com/tokio-rs/mio/issues/1527
            local_addr: create_null_socketaddr(),
        })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => accept_remote(AcceptedType::Remote(
                    create_null_socketaddr(), // TODO: provide correct address
                    StreamRemoteResource { stream },
                )),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("unix socket accept error: {}", err), // Should not happen
            }
        }
    }

    // nearly impossible to implement
    // fn send_to(&self, addr: SocketAddr, data: &[u8]) -> SendStatus {
    //        
    // }
}

pub(crate) struct DatagramRemoteResource {
    datagram: UnixDatagram
}

impl Resource for DatagramRemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.datagram
    }
}

impl Remote for DatagramRemoteResource {
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        let config = match config {
            TransportConnect::UnixSocketDatagram(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        let datagram = UnixDatagram::unbound()?;
        datagram.connect(config.path)?;
        
        Ok(ConnectionInfo {
            local_addr: create_null_socketaddr(),
            peer_addr: create_null_socketaddr(),
            remote: Self {
                datagram
            }
        })
    }

    // A majority of send, reciev and accept in local are reused from udp.rs due to similarities

    fn receive(&self, mut process_data: impl FnMut(&[u8])) -> ReadStatus {
        let buffer: MaybeUninit<[u8; MAX_PAYLOAD_LEN]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.datagram.recv(&mut input_buffer) {
                Ok(size) => process_data(&mut input_buffer[..size]),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
                    break ReadStatus::WaitNextEvent
                }
                Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                    // Avoid ICMP generated error to be logged
                    break ReadStatus::WaitNextEvent
                }
                Err(err) => {
                    log::error!("unix datagram socket receive error: {}", err);
                    break ReadStatus::WaitNextEvent // Should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        loop {
            match self.datagram.send(data) {
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
                    log::error!("unix datagram socket send error: {}", err);
                    break SendStatus::ResourceNotFound // should not happen
                }
            }
        }
    }

    fn pending(&self, readiness: Readiness) -> PendingStatus {
        PendingStatus::Ready
    }
}
// datagram is also used for listener
pub(crate) struct DatagramLocalResource {
    listener: UnixDatagram,
    bind_path: PathBuf
}

impl Resource for DatagramLocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.listener
    }
}


impl Local for DatagramLocalResource {
    type Remote = DatagramRemoteResource;

    fn listen_with(config: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let config = match config {
            TransportListen::UnixDatagramSocket(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        let listener = UnixDatagram::bind(&config.path)?;

        Ok(ListeningInfo {
            local: Self {
                listener,
                bind_path: config.path
            },
            local_addr: create_null_socketaddr(),
        })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        let buffer: MaybeUninit<[u8; MAX_PAYLOAD_LEN]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            match self.listener.recv_from(&mut input_buffer) {
                Ok((size, addr)) => {
                    let data = &mut input_buffer[..size];
                    accept_remote(AcceptedType::Data(create_null_socketaddr(), data))
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => break log::error!("Unix datagram socket accept error: {}", err), // Should never happen
            };
        }
    }
}

impl Drop for DatagramLocalResource {
    fn drop(&mut self) {
        // this may fail if the file is already removed
        match fs::remove_file(&self.bind_path) {
            Ok(_) => (),
            Err(err) => log::error!("Error removing unix socket file on drop: {}", err),
        }
    }
}

pub(crate) struct UnixSocketDatagramAdapter;
impl Adapter for UnixSocketDatagramAdapter {
    type Remote = DatagramRemoteResource;
    type Local = DatagramLocalResource;
}
