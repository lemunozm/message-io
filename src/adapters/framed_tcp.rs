pub use socket2::{TcpKeepalive};

use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};
use crate::util::encoding::{self, Decoder, MAX_ENCODED_SIZE};

use mio::net::{TcpListener, TcpStream};
use mio::event::{Source};

use socket2::{Socket};

use std::net::{SocketAddr};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};
use std::cell::{RefCell};
use std::mem::{forget, MaybeUninit};
#[cfg(target_os = "windows")]
use std::os::windows::io::{FromRawSocket, AsRawSocket};
#[cfg(not(target_os = "windows"))]
use std::os::{fd::AsRawFd, unix::io::FromRawFd};

const INPUT_BUFFER_SIZE: usize = u16::MAX as usize; // 2^16 - 1

#[derive(Clone, Debug, Default)]
pub struct FramedTcpConnectConfig {
    keepalive: Option<TcpKeepalive>,
}

impl FramedTcpConnectConfig {
    /// Enables TCP keepalive settings on the socket.
    pub fn with_keepalive(mut self, keepalive: TcpKeepalive) -> Self {
        self.keepalive = Some(keepalive);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct FramedTcpListenConfig {
    keepalive: Option<TcpKeepalive>,
}

impl FramedTcpListenConfig {
    /// Enables TCP keepalive settings on client connection sockets.
    pub fn with_keepalive(mut self, keepalive: TcpKeepalive) -> Self {
        self.keepalive = Some(keepalive);
        self
    }
}

pub(crate) struct FramedTcpAdapter;
impl Adapter for FramedTcpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub(crate) struct RemoteResource {
    stream: TcpStream,
    decoder: RefCell<Decoder>,
    keepalive: Option<TcpKeepalive>,
}

// SAFETY:
// That RefCell<Decoder> can be used with Sync because the decoder is only used in the read_event,
// that will be called always from the same thread. This way, we save the cost of a Mutex.
unsafe impl Sync for RemoteResource {}

impl RemoteResource {
    fn new(stream: TcpStream, keepalive: Option<TcpKeepalive>) -> Self {
        Self { stream, decoder: RefCell::new(Decoder::default()), keepalive }
    }
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
            TransportConnect::FramedTcp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };
        let peer_addr = *remote_addr.socket_addr();
        let stream = TcpStream::connect(peer_addr)?;
        let local_addr = stream.local_addr()?;
        Ok(ConnectionInfo {
            remote: RemoteResource::new(stream, config.keepalive),
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
                Ok(size) => {
                    let data = &input_buffer[..size];
                    log::trace!("Decoding {} bytes", data.len());
                    self.decoder.borrow_mut().decode(data, |decoded_data| {
                        process_data(decoded_data);
                    });
                }
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
        let mut buf = [0; MAX_ENCODED_SIZE]; // used to avoid a heap allocation
        let encoded_size = encoding::encode_size(data, &mut buf);

        let mut total_bytes_sent = 0;
        let total_bytes = encoded_size.len() + data.len();
        loop {
            let data_to_send = match total_bytes_sent < encoded_size.len() {
                true => &encoded_size[total_bytes_sent..],
                false => &data[total_bytes_sent - encoded_size.len()..],
            };

            let stream = &self.stream;
            match stream.deref().write(data_to_send) {
                Ok(bytes_sent) => {
                    total_bytes_sent += bytes_sent;
                    if total_bytes_sent == total_bytes {
                        break SendStatus::Sent
                    }
                }
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                Err(err) => {
                    log::error!("TCP receive error: {}", err);
                    break SendStatus::ResourceNotFound // should not happen
                }
            }
        }
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        let status = super::tcp::check_stream_ready(&self.stream);

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
            TransportListen::FramedTcp(config) => config,
            _ => panic!("Internal error: Got wrong config"),
        };
        let listener = TcpListener::bind(addr)?;
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
                    RemoteResource::new(stream, self.keepalive.clone()),
                )),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("TCP accept error: {}", err), // Should not happen
            }
        }
    }
}
