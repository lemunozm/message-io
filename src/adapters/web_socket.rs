use crate::adapter::{Resource, Adapter, ActionHandler, EventHandler, SendStatus, AcceptedType, ReadStatus};
use crate::util::{OTHER_THREAD_ERR};

use mio::event::{Source};
use mio::net::{TcpStream, TcpListener};

use tungstenite::protocol::{WebSocket, Role, Message};
use tungstenite::server::{accept as ws_accept};
use tungstenite::error::{Error};

use std::sync::{Mutex};
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind};

pub struct ClientResource(Mutex<WebSocket<TcpStream>>);
impl Resource for ClientResource {
    fn source(&mut self) -> &mut dyn Source {
        self.0.get_mut().unwrap().get_mut()
    }
}

pub struct ServerResource(TcpListener);
impl Resource for ServerResource {
    fn source(&mut self) -> &mut dyn Source {
        &mut self.0
    }
}

pub struct WsAdapter;
impl Adapter for WsAdapter {
    type Remote = ClientResource;
    type Listener = ServerResource;
    type ActionHandler = WsActionHandler;
    type EventHandler = WsEventHandler;

    fn split(self) -> (WsActionHandler, WsEventHandler) {
        (WsActionHandler, WsEventHandler)
    }
}

pub struct WsActionHandler;
impl ActionHandler for WsActionHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<ClientResource> {
        let stream = StdTcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        let stream = TcpStream::from_std(stream);
        Ok(ClientResource(Mutex::new(WebSocket::from_raw_socket(stream, Role::Client, None))))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ServerResource, SocketAddr)> {
        let listener = TcpListener::bind(addr)?;
        let real_addr = listener.local_addr().unwrap();
        Ok((ServerResource(listener), real_addr))
    }

    fn send(&mut self, web_socket: &ClientResource, data: &[u8]) -> SendStatus {
        let message = Message::Binary(data.to_vec());
        let mut socket = web_socket.0.lock().expect(OTHER_THREAD_ERR);
        let mut result = socket.write_message(message);
        loop {
            match result {
                Ok(_) => break SendStatus::Sent,
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock  => {
                    result = socket.write_pending();
                }
                Err(_) => break SendStatus::ResourceNotFound,
            }
        }
    }
}

pub struct WsEventHandler;
impl EventHandler for WsEventHandler {
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
                Ok((stream, addr)) => {
                    let resource = ClientResource(Mutex::new(ws_accept(stream).unwrap()));
                    accept_remote(AcceptedType::Remote(addr, resource));
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break log::trace!("WebSocket accept event error"), // Should not happen
            }
        }
    }

    fn read_event(
        &mut self,
        resource: &ClientResource,
        process_data: &dyn Fn(&[u8])
    ) -> ReadStatus {
        loop {
            match resource.0.lock().expect(OTHER_THREAD_ERR).read_message() {
                Ok(message) => match message {
                    Message::Binary(data) => process_data(&data),
                    Message::Close(_) => break ReadStatus::Disconnected,
                    _ => continue,
                }
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock => {
                    break ReadStatus::WaitNextEvent
                }
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::ConnectionReset => {
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
