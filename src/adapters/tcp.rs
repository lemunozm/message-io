pub use socket2::{TcpKeepalive};

use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};

use mio::net::{TcpListener, TcpStream};
use mio::event::{Source};

use socket2::{Socket, Domain, Type, Protocol};

use std::net::{SocketAddr};
#[cfg(unix)]
use std::ffi::{CString};
use std::io::{self, ErrorKind, Read, Write};
#[cfg(target_os = "macos")]
use std::num::NonZeroU32;
use std::ops::{Deref};
use std::mem::{forget, MaybeUninit};
use std::os::raw::c_int;
#[cfg(target_os = "windows")]
use std::os::windows::io::{FromRawSocket, AsRawSocket};
#[cfg(not(target_os = "windows"))]
use std::os::{fd::AsRawFd, unix::io::FromRawFd};

/// Size of the internal reading buffer.
/// It implies that at most the generated [`crate::network::NetEvent::Message`]
/// will contains a chunk of data of this value.
pub const INPUT_BUFFER_SIZE: usize = u16::MAX as usize; // 2^16 - 1

/// The maximum length of the pending (unaccepted) connection queue of a listener.
pub const LISTENER_BACKLOG: c_int = 1024;

#[derive(Clone, Debug, Default)]
pub struct TcpConnectConfig {
    bind_device: Option<String>,
    source_address: Option<SocketAddr>,
    keepalive: Option<TcpKeepalive>,
}

impl TcpConnectConfig {
    /// Bind the TCP connection to a specific interface, identified by its name. This option works
    /// in Unix, on other systems, it will be ignored.
    pub fn with_bind_device(mut self, device: String) -> Self {
        self.bind_device = Some(device);
        self
    }

    /// Enables TCP keepalive settings on the socket.
    pub fn with_keepalive(mut self, keepalive: TcpKeepalive) -> Self {
        self.keepalive = Some(keepalive);
        self
    }

    /// Specify the source address and port.
    pub fn with_source_address(mut self, source_address: SocketAddr) -> Self {
        self.source_address = Some(source_address);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct TcpListenConfig {
    bind_device: Option<String>,
    keepalive: Option<TcpKeepalive>,
}

impl TcpListenConfig {
    /// Bind the TCP listener to a specific interface, identified by its name. This option works in
    /// Unix, on other systems, it will be ignored.
    pub fn with_bind_device(mut self, device: String) -> Self {
        self.bind_device = Some(device);
        self
    }

    /// Enables TCP keepalive settings on client connection sockets.
    pub fn with_keepalive(mut self, keepalive: TcpKeepalive) -> Self {
        self.keepalive = Some(keepalive);
        self
    }
}

pub(crate) struct TcpAdapter;
impl Adapter for TcpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub(crate) struct RemoteResource {
    stream: TcpStream,
    keepalive: Option<TcpKeepalive>,
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.stream
    }
}

impl Remote for RemoteResource {
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        let config = match config {
            TransportConnect::Tcp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };
        let peer_addr = *remote_addr.socket_addr();

        let socket = Socket::new(
            match peer_addr {
                SocketAddr::V4 { .. } => Domain::IPV4,
                SocketAddr::V6 { .. } => Domain::IPV6,
            },
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        socket.set_nonblocking(true)?;

        if let Some(source_address) = config.source_address {
            socket.bind(&source_address.into())?;
        }

        #[cfg(unix)]
        if let Some(bind_device) = config.bind_device {
            let device = CString::new(bind_device)?;

            #[cfg(not(target_os = "macos"))]
            socket.bind_device(Some(device.as_bytes()))?;

            #[cfg(target_os = "macos")]
            match NonZeroU32::new(unsafe { libc::if_nametoindex(device.as_ptr()) }) {
                Some(index) => socket.bind_device_by_index(Some(index))?,
                None => {
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "Bind device interface not found",
                    ))
                }
            }
        }

        match socket.connect(&peer_addr.into()) {
            #[cfg(unix)]
            Err(e) if e.raw_os_error() != Some(libc::EINPROGRESS) => return Err(e),
            #[cfg(windows)]
            Err(e) if e.kind() != io::ErrorKind::WouldBlock => return Err(e),
            _ => {}
        }

        let stream = TcpStream::from_std(socket.into());
        let local_addr = stream.local_addr()?;
        Ok(ConnectionInfo {
            remote: Self { stream, keepalive: config.keepalive },
            local_addr,
            peer_addr,
        })
    }

    fn receive(&self, mut process_data: impl FnMut(&[u8])) -> ReadStatus {
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
                    log::error!("TCP receive error: {}", err);
                    break ReadStatus::Disconnected // should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        // TODO: The current implementation implies an active waiting,
        // improve it using POLLIN instead to avoid active waiting.
        // Note: Despite the fear that an active waiting could generate,
        // this only occurs in the case when the receiver is full because reads slower that it sends.
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
                    log::error!("TCP receive error: {}", err);
                    break SendStatus::ResourceNotFound // should not happen
                }
            }
        }
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        let status = check_stream_ready(&self.stream);

        if status == PendingStatus::Ready {
            if let Some(keepalive) = &self.keepalive {
                #[cfg(target_os = "windows")]
                let socket = unsafe { Socket::from_raw_socket(self.stream.as_raw_socket()) };
                #[cfg(not(target_os = "windows"))]
                let socket = unsafe { Socket::from_raw_fd(self.stream.as_raw_fd()) };

                if let Err(e) = socket.set_tcp_keepalive(keepalive) {
                    log::warn!("TCP set keepalive error: {}", e);
                }

                // Don't drop so the underlying socket is not closed.
                forget(socket);
            }
        }

        status
    }
}

/// Check if a TcpStream can be considered connected.
pub fn check_stream_ready(stream: &TcpStream) -> PendingStatus {
    // A multiplatform non-blocking way to determine if the TCP stream is connected:
    // Extracted from: https://github.com/tokio-rs/mio/issues/1486
    if let Ok(Some(_)) = stream.take_error() {
        return PendingStatus::Disconnected
    }
    match stream.peer_addr() {
        Ok(_) => PendingStatus::Ready,
        Err(err) if err.kind() == io::ErrorKind::NotConnected => PendingStatus::Incomplete,
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => PendingStatus::Incomplete,
        Err(_) => PendingStatus::Disconnected,
    }
}

pub(crate) struct LocalResource {
    listener: TcpListener,
    keepalive: Option<TcpKeepalive>,
}

impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.listener
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen_with(config: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let config = match config {
            TransportListen::Tcp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };

        let socket = Socket::new(
            match addr {
                SocketAddr::V4 { .. } => Domain::IPV4,
                SocketAddr::V6 { .. } => Domain::IPV6,
            },
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        socket.set_nonblocking(true)?;
        socket.set_reuse_address(true)?;

        #[cfg(unix)]
        if let Some(bind_device) = config.bind_device {
            let device = CString::new(bind_device)?;

            #[cfg(not(target_os = "macos"))]
            socket.bind_device(Some(device.as_bytes()))?;

            #[cfg(target_os = "macos")]
            match NonZeroU32::new(unsafe { libc::if_nametoindex(device.as_ptr()) }) {
                Some(index) => socket.bind_device_by_index(Some(index))?,
                None => {
                    return Err(io::Error::new(
                        ErrorKind::NotFound,
                        "Bind device interface not found",
                    ))
                }
            }
        }

        socket.bind(&addr.into())?;
        socket.listen(LISTENER_BACKLOG)?;

        let listener = TcpListener::from_std(socket.into());

        let local_addr = listener.local_addr().unwrap();
        Ok(ListeningInfo {
            local: { LocalResource { listener, keepalive: config.keepalive } },
            local_addr,
        })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => accept_remote(AcceptedType::Remote(
                    addr,
                    RemoteResource { stream, keepalive: self.keepalive.clone() },
                )),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("TCP accept error: {}", err), // Should not happen
            }
        }
    }
}
