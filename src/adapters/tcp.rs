use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::adapter::{AdapterEvent, Adapter, Controller, Processor};
use crate::poll::{PollRegister};
use crate::encoding::{self, DecodingPool};
use crate::util::{OTHER_THREAD_ERR, SendingStatus};

use mio::net::{TcpListener, TcpStream};

use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};

const INPUT_BUFFER_SIZE: usize = 65535; // 2^16 - 1

struct Store {
    // We store the addr because we will need it when the stream crash.
    // When a stream crash by an error (i.e. reset) peer_addr no longer returns the addr.
    streams: RwLock<HashMap<ResourceId, (TcpStream, SocketAddr)>>,
    listeners: RwLock<HashMap<ResourceId, TcpListener>>,
}

impl Store {
    fn new() -> Store {
        Store { streams: RwLock::new(HashMap::new()), listeners: RwLock::new(HashMap::new()) }
    }
}

pub struct TcpAdapter;

impl<C> Adapter<C> for TcpAdapter
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    type Controller = TcpController;
    type Processor = TcpProcessor;

    fn split(&self, poll_register: PollRegister) -> (TcpController, TcpProcessor) {
        let store = Arc::new(Store::new());
        (
            TcpController::new(store.clone(), poll_register.clone()),
            TcpProcessor::new(store, poll_register),
        )
    }
}

pub struct TcpController {
    store: Arc<Store>,
    poll_register: PollRegister,
}

impl TcpController {
    fn new(store: Arc<Store>, poll_register: PollRegister) -> Self {
        Self { store, poll_register }
    }
}

impl Controller for TcpController {
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint> {
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        let mut stream = TcpStream::from_std(stream);

        let id = self.poll_register.add(&mut stream, ResourceType::Remote);
        self.store.streams.write().expect(OTHER_THREAD_ERR).insert(id, (stream, addr));
        Ok(Endpoint::new(id, addr))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let mut listener = TcpListener::bind(addr)?;

        let id = self.poll_register.add(&mut listener, ResourceType::Listener);
        let real_addr = listener.local_addr().unwrap();
        self.store.listeners.write().expect(OTHER_THREAD_ERR).insert(id, listener);
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
                .streams
                .write()
                .expect(OTHER_THREAD_ERR)
                .remove(&id)
                .map(|(mut stream, _)| poll_register.remove(&mut stream)),
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
                .streams
                .read()
                .expect(OTHER_THREAD_ERR)
                .get(&id)
                .map(|(stream, _)| stream.local_addr().unwrap()),
        }
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus {
        let streams = self.store.streams.read().expect(OTHER_THREAD_ERR);
        match streams.get(&endpoint.resource_id()) {
            Some((stream, _)) => {
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
            None => SendingStatus::RemovedEndpoint,
        }
    }
}

pub struct TcpProcessor {
    store: Arc<Store>,
    poll_register: PollRegister,
    decoding_pool: DecodingPool<Endpoint>,
    input_buffer: [u8; INPUT_BUFFER_SIZE],
}

impl TcpProcessor {
    fn new(store: Arc<Store>, poll_register: PollRegister) -> Self {
        Self {
            store,
            poll_register,
            input_buffer: [0; INPUT_BUFFER_SIZE],
            decoding_pool: DecodingPool::new(),
        }
    }
}

impl<C> Processor<C> for TcpProcessor
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    fn process_listener(&mut self, id: ResourceId, event_callback: &mut C) {
        // We check the existance of the listener because some event could be produced
        // before removing it.
        if let Some(listener) = self.store.listeners.read().expect(OTHER_THREAD_ERR).get(&id) {
            let mut streams = self.store.streams.write().expect(OTHER_THREAD_ERR);
            loop {
                match listener.accept() {
                    Ok((mut stream, addr)) => {
                        let id = self.poll_register.add(&mut stream, ResourceType::Remote);
                        streams.insert(id, (stream, addr));

                        let endpoint = Endpoint::new(id, addr);
                        event_callback(endpoint, AdapterEvent::Added);
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        log::trace!("TCP process listener error");
                        break // should not happen
                    }
                }
            }
        }
    }

    fn process_remote(&mut self, id: ResourceId, event_callback: &mut C) {
        let streams = self.store.streams.read().expect(OTHER_THREAD_ERR);
        if let Some((stream, addr)) = streams.get(&id) {
            let endpoint = Endpoint::new(id, *addr);
            let must_be_removed = loop {
                match stream.deref().read(&mut self.input_buffer) {
                    Ok(0) => break true,
                    Ok(size) => {
                        let data = &self.input_buffer[..size];
                        log::trace!("Decoding data from {}, {} bytes", endpoint, data.len());
                        self.decoding_pool.decode_from(data, endpoint, |decoded_data| {
                            event_callback(endpoint, AdapterEvent::Data(decoded_data));
                        });
                    }
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break false,
                    Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        log::error!("TCP process stream error");
                        break true // should not happen
                    }
                }
            };

            if must_be_removed {
                let poll_register = &mut self.poll_register;
                drop(streams);
                self.store
                    .streams
                    .write()
                    .expect(OTHER_THREAD_ERR)
                    .remove(&id)
                    .map(|(mut stream, _)| {
                        poll_register.remove(&mut stream);
                    })
                    .unwrap();
                self.decoding_pool.remove_if_exists(endpoint);
                event_callback(endpoint, AdapterEvent::Removed);
            }
        }
    }
}
