use crate::adapter::{
    Resource, Adapter, ActionHandler, EventHandler, SendStatus, AcceptedType, ReadStatus,
};
use crate::remote_addr::{RemoteAddr};
use crate::util::{OTHER_THREAD_ERR};

use mio::event::{Source};
use mio::net::{TcpStream, TcpListener};

use tungstenite::protocol::{WebSocket, Message};
use tungstenite::server::{accept as ws_accept};
use tungstenite::client::{client as ws_connect};
use tungstenite::handshake::{HandshakeError};
use tungstenite::error::{Error};

use url::Url;

use std::sync::{Mutex};
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind};

/// Max message size
// From https://docs.rs/tungstenite/0.13.0/src/tungstenite/protocol/mod.rs.html#65
pub const MAX_WS_PAYLOAD_LEN: usize = 64 << 20;

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

    fn connect(&mut self, remote_addr: RemoteAddr) -> io::Result<(ClientResource, SocketAddr)> {
        let (addr, url) = match remote_addr {
            RemoteAddr::SocketAddr(addr) => (addr, Url::parse(&format!("ws://{}/message-io-default", addr)).unwrap()),
            RemoteAddr::Url(url) => {
                let addr = url.socket_addrs(|| match url.scheme() {
                    "ws" => Some(80), // Plain
                    "wss" => Some(443), //Tls
                    _ => None,
                }).unwrap()[0];
                (addr, url)
            }
        };
        // Synchronous tcp handshake
        let stream = StdTcpStream::connect(addr)?;

        // Make it an asynchronous mio TcpStream
        stream.set_nonblocking(true)?;
        let stream = TcpStream::from_std(stream);

        // Synchronous waiting for web socket handshake
        let mut handshake_result = ws_connect(url, stream);
        loop {
            match handshake_result {
                Ok((ws_socket, _)) => break Ok((ClientResource(Mutex::new(ws_socket)), addr)),
                Err(HandshakeError::Interrupted(mid_handshake)) => {
                    handshake_result = mid_handshake.handshake();
                }
                Err(HandshakeError::Failure(error)) => panic!("Unexpected ws error: {}", error),
            }
        }
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
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock => {
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
        process_data: &dyn Fn(&[u8]),
    ) -> ReadStatus
    {
        loop {
            match resource.0.lock().expect(OTHER_THREAD_ERR).read_message() {
                Ok(message) => match message {
                    Message::Binary(data) => process_data(&data),
                    Message::Close(_) => break ReadStatus::Disconnected,
                    _ => continue,
                },
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
