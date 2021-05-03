use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo,
};
use crate::network::{RemoteAddr};

use mio::net::{TcpListener, TcpStream};
use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};
use std::mem::{MaybeUninit};

/// Size of the internal reading buffer.
/// It implies that at most the generated [`crate::network::NetEvent::Message`]
/// will contains a chunk of data of this value.
pub const INPUT_BUFFER_SIZE: usize = u16::MAX as usize; // 2^16 - 1

pub(crate) struct TcpAdapter;
impl Adapter for TcpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub(crate) struct RemoteResource {
    stream: TcpStream,
}

impl From<TcpStream> for RemoteResource {
    fn from(stream: TcpStream) -> Self {
        Self { stream }
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
        let stream = TcpStream::connect(peer_addr)?;
        let local_addr = stream.local_addr()?;
        Ok(ConnectionInfo { remote: stream.into(), local_addr, peer_addr })
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
}

pub(crate) struct LocalResource {
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

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
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
