use crate::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo,
};
use crate::remote_addr::{RemoteAddr};
use crate::encoding::{self, Decoder};

use mio::net::{TcpListener, TcpStream};
use mio::event::{Source};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};
use std::cell::{RefCell};
use std::mem::{MaybeUninit};

const INPUT_BUFFER_SIZE: usize = 65535; // 2^16 - 1

/// The max packet value for tcp.
/// Although this size is very high, it is preferred send data in smaller chunks with a rate
/// to not saturate the receiver thread in the endpoint.
pub const MAX_TCP_PAYLOAD_LEN: usize = usize::MAX;

pub struct FramedTcpAdapter;
impl Adapter for FramedTcpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub struct RemoteResource {
    stream: TcpStream,
    decoder: RefCell<Decoder>,
}

/// That RefCell<Decoder> can be used with Sync because it is only used in the read_event.
/// This way, we save the cost of a Mutex.
unsafe impl Sync for RemoteResource {}

impl From<TcpStream> for RemoteResource {
    fn from(stream: TcpStream) -> Self {
        Self { stream, decoder: RefCell::new(Decoder::default()) }
    }
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.stream
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<ConnectionInfo<Self>> {
        let peer_addr = *remote_addr.socket_addr();
        let stream = StdTcpStream::connect(peer_addr)?;
        let local_addr = stream.local_addr()?;
        stream.set_nonblocking(true)?;
        Ok(ConnectionInfo { remote: TcpStream::from_std(stream).into(), local_addr, peer_addr })
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        let buffer: MaybeUninit<[u8; INPUT_BUFFER_SIZE]> = MaybeUninit::uninit();
        let mut input_buffer = unsafe { buffer.assume_init() }; // Avoid to initialize the array

        loop {
            let stream = &self.stream;
            match stream.deref().read(&mut input_buffer) {
                Ok(0) => break ReadStatus::Disconnected,
                Ok(size) => {
                    let data = &input_buffer[..size];
                    let addr = self.stream.peer_addr().unwrap();
                    log::trace!("Decoding data from {}, {} bytes", addr, data.len());
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
        let encoded_size = encoding::encode_size(data);

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
}

pub struct LocalResource {
    listener: TcpListener,
}

impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.listener
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen(addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let listener = TcpListener::bind(addr)?;
        let local_addr = listener.local_addr().unwrap();
        Ok(ListeningInfo { local: { LocalResource { listener } }, local_addr })
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => accept_remote(AcceptedType::Remote(addr, stream.into())),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("TCP accept error: {}", err), // Should not happen
            }
        }
    }
}
