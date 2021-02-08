use crate::endpoint::{Endpoint};
use crate::adapter::{Adapter, ActionHandler, EventHandler, AcceptionEvent};
use crate::encoding::{self, DecodingPool};
use crate::util::{SendingStatus, OTHER_THREAD_ERR};

use mio::net::{TcpListener, TcpStream};

use std::sync::{Arc, Mutex};
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};

const INPUT_BUFFER_SIZE: usize = 65535; // 2^16 - 1

pub struct TcpAdapter;

impl Adapter for TcpAdapter {
    type Remote = TcpStream;
    type Listener = TcpListener;
    type ActionHandler = TcpActionHandler;
    type EventHandler = TcpEventHandler;

    fn split(self) -> (TcpActionHandler, TcpEventHandler) {
        let decoding_pool = Arc::new(Mutex::new(DecodingPool::new()));
        (TcpActionHandler::new(decoding_pool.clone()), TcpEventHandler::new(decoding_pool))
    }
}

pub struct TcpActionHandler {
    decoding_pool: Arc<Mutex<DecodingPool<SocketAddr>>>,
}

impl TcpActionHandler {
    fn new(decoding_pool: Arc<Mutex<DecodingPool<SocketAddr>>>) -> Self {
        Self { decoding_pool }
    }
}

impl ActionHandler for TcpActionHandler {
    type Remote = TcpStream;
    type Listener = TcpListener;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<TcpStream> {
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        Ok(TcpStream::from_std(stream))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(TcpListener, SocketAddr)> {
        let listener = TcpListener::bind(addr)?;
        let real_addr = listener.local_addr().unwrap();
        Ok((listener, real_addr))
    }

    fn send(&mut self, stream: &TcpStream, _: Endpoint, data: &[u8]) -> SendingStatus {
        let encode_value = encoding::encode(data);

        // TODO: The current implementation implies an active waiting,
        // improve it using POLLIN instead to avoid active waiting.
        // Note: Despite the fear that an active waiting could generate,
        // this waiting only occurs in the rare case when the send method needs block.
        let mut total_bytes_sent = 0;
        let total_bytes = encoding::PADDING + data.len();
        loop {
            let data_to_send = match total_bytes_sent < encoding::PADDING {
                true => &encode_value[total_bytes_sent..],
                false => &data[(total_bytes_sent - encoding::PADDING)..],
            };

            match stream.deref().write(data_to_send) {
                Ok(bytes_sent) => {
                    total_bytes_sent += bytes_sent;
                    if total_bytes_sent == total_bytes {
                        break SendingStatus::Sent
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
                Err(_) => break SendingStatus::RemovedEndpoint,
            }
        }
    }

    fn remove_remote(&mut self, _resource: Self::Remote, addr: SocketAddr) {
        self.decoding_pool.lock().expect(OTHER_THREAD_ERR).remove_if_exists(addr);
    }
}

pub struct TcpEventHandler {
    decoding_pool: Arc<Mutex<DecodingPool<SocketAddr>>>,
    input_buffer: [u8; INPUT_BUFFER_SIZE],
}

impl TcpEventHandler {
    fn new(decoding_pool: Arc<Mutex<DecodingPool<SocketAddr>>>) -> Self {
        Self { input_buffer: [0; INPUT_BUFFER_SIZE], decoding_pool }
    }
}

impl EventHandler for TcpEventHandler {
    type Remote = TcpStream;
    type Listener = TcpListener;

    fn accept_event(
        &mut self,
        listener: &TcpListener,
        event_callback: &mut dyn FnMut(AcceptionEvent<'_, TcpStream>),
    )
    {
        loop {
            match listener.accept() {
                Ok((stream, addr)) => event_callback(AcceptionEvent::Remote(addr, stream)),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    log::trace!("TCP process listener error");
                    break // should not happen
                }
            }
        }
    }

    fn read_event(
        &mut self,
        stream: &TcpStream,
        addr: SocketAddr,
        event_callback: &mut dyn FnMut(&[u8]),
    ) -> bool
    {
        let removed = loop {
            match stream.deref().read(&mut self.input_buffer) {
                Ok(0) => break true,
                Ok(size) => {
                    let data = &self.input_buffer[..size];
                    log::trace!("Decoding data from {}, {} bytes", addr, data.len());
                    self.decoding_pool.lock().expect(OTHER_THREAD_ERR).decode_from(
                        data,
                        addr,
                        |decoded_data| {
                            event_callback(decoded_data);
                        },
                    );
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break false,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    log::error!("TCP process stream error");
                    break true // should not happen
                }
            }
        };

        if removed {
            self.decoding_pool.lock().expect(OTHER_THREAD_ERR).remove_if_exists(addr);
        }

        removed
    }
}
