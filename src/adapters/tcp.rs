use crate::adapter::{Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus};
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

pub struct TcpAdapter;
impl Adapter for TcpAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub struct RemoteResource {
    stream: TcpStream,
    decoder: RefCell<Decoder>,
}

/// We are totally sure that RefCell<Decoder> can be used with Sync
/// because it is only used in the read_event.
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
    fn connect(remote_addr: RemoteAddr) -> io::Result<(Self, SocketAddr)> {
        let addr = remote_addr.socket_addr();
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        Ok((TcpStream::from_std(stream).into(), *addr))
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

        // TODO: The current implementation implies an active waiting,
        // improve it using POLLIN instead to avoid active waiting.
        // Note: Despite the fear that an active waiting could generate,
        // this waiting only occurs in the rare case when the send method needs block.
        let mut total_bytes_sent = 0;
        let total_bytes = encoding::PADDING + data.len();
        loop {
            let data_to_send = match total_bytes_sent < encoding::PADDING {
                true => &encoded_size[total_bytes_sent..],
                false => &data[(total_bytes_sent - encoding::PADDING)..],
            };

            let stream = &self.stream;
            match stream.deref().write(data_to_send) {
                Ok(bytes_sent) => {
                    total_bytes_sent += bytes_sent;
                    if total_bytes_sent == total_bytes {
                        break SendStatus::Sent
                    }
                    // We get sending to data, but not the totality.
                    // We start waiting actively.
                }

                // If WouldBlock is received in this non-blocking socket means that
                // the sending buffer is full and it should wait to send more data.
                // This occurs when huge amounts of data are sent and It could be
                // intensified if the remote endpoint reads slower than this enpoint sends.
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => continue,

                // Others errors are considered fatal for the connection.
                // an Event::Disconnection will be generated later.
                // It is possible to reach this point if the sending method is produced
                // before the disconnection/reset event is generated.
                Err(err) => {
                    log::error!("TCP send error: {}", err);
                    break SendStatus::ResourceNotFound
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

    fn listen(addr: SocketAddr) -> io::Result<(Self, SocketAddr)> {
        let listener = TcpListener::bind(addr)?;
        let real_addr = listener.local_addr().unwrap();
        Ok((LocalResource { listener }, real_addr))
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => accept_remote(AcceptedType::Remote(addr, stream.into())),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::trace!("TCP accept error: {}", err), // Should not happen
            }
        }
    }
}
