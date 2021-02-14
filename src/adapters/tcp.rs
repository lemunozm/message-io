use crate::adapter::{
    Resource, Adapter, ActionHandler, EventHandler, SendStatus, AcceptedType, ReadStatus,
};
use crate::encoding::{self, Decoder};

use mio::net::{TcpListener, TcpStream};
use mio::event::{Source};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};
use std::cell::{RefCell};

const INPUT_BUFFER_SIZE: usize = 65535; // 2^16 - 1

pub struct ClientResource {
    stream: TcpStream,
    decoder: RefCell<Decoder>,
}

/// We are totally sure that RefCell<Decoder> can be used with Sync
/// because it is only used in the read_event.
/// This way, we save the cost of a Mutex.
unsafe impl Sync for ClientResource {}

impl Resource for ClientResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.stream
    }
}

impl From<TcpStream> for ClientResource {
    fn from(stream: TcpStream) -> Self {
        Self { stream, decoder: RefCell::new(Decoder::default()) }
    }
}

pub struct ServerResource(TcpListener);
impl Resource for ServerResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.0
    }
}

pub struct TcpAdapter;
impl Adapter for TcpAdapter {
    type Remote = ClientResource;
    type Listener = ServerResource;
    type ActionHandler = TcpActionHandler;
    type EventHandler = TcpEventHandler;

    fn split(self) -> (TcpActionHandler, TcpEventHandler) {
        (TcpActionHandler, TcpEventHandler::default())
    }
}

pub struct TcpActionHandler;
impl ActionHandler for TcpActionHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<ClientResource> {
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        Ok(TcpStream::from_std(stream).into())
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ServerResource, SocketAddr)> {
        let listener = TcpListener::bind(addr)?;
        let real_addr = listener.local_addr().unwrap();
        Ok((ServerResource(listener), real_addr))
    }

    fn send(&mut self, remote: &ClientResource, data: &[u8]) -> SendStatus {
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

            let stream = &remote.stream;
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
                Err(_) => break SendStatus::ResourceNotFound,
            }
        }
    }
}

pub struct TcpEventHandler {
    input_buffer: [u8; INPUT_BUFFER_SIZE],
}

impl Default for TcpEventHandler {
    fn default() -> Self {
        Self { input_buffer: [0; INPUT_BUFFER_SIZE] }
    }
}

impl EventHandler for TcpEventHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn accept_event(
        &mut self,
        listener: &ServerResource,
        accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>),
    )
    {
        loop {
            match listener.0.accept() {
                Ok((stream, addr)) => accept_remote(AcceptedType::Remote(addr, stream.into())),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break log::trace!("TCP accept event error"), // Should not happen
            }
        }
    }

    fn read_event(&mut self, remote: &ClientResource, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        loop {
            let stream = &remote.stream;
            match stream.deref().read(&mut self.input_buffer) {
                Ok(0) => break ReadStatus::Disconnected,
                Ok(size) => {
                    let data = &self.input_buffer[..size];
                    let addr = remote.stream.peer_addr().unwrap();
                    log::trace!("Decoding data from {}, {} bytes", addr, data.len());
                    remote.decoder.borrow_mut().decode(data, |decoded_data| {
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
                Err(_) => {
                    log::error!("TCP read event error");
                    break ReadStatus::Disconnected // should not happen
                }
            }
        }
    }
}
