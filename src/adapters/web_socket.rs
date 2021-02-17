use crate::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo,
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

pub struct WsAdapter;
impl Adapter for WsAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub struct RemoteResource {
    web_socket: Mutex<WebSocket<TcpStream>>,
}

impl From<WebSocket<TcpStream>> for RemoteResource {
    fn from(web_socket: WebSocket<TcpStream>) -> Self {
        Self { web_socket: Mutex::new(web_socket) }
    }
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        // We return safety the inner TcpStream without blocking
        self.web_socket.get_mut().unwrap().get_mut()
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<ConnectionInfo<Self>> {
        let (peer_addr, url) = match remote_addr {
            RemoteAddr::SocketAddr(addr) => {
                (addr, Url::parse(&format!("ws://{}/message-io-default", addr)).unwrap())
            }
            RemoteAddr::Url(url) => {
                let addr = url
                    .socket_addrs(|| match url.scheme() {
                        "ws" => Some(80),   // Plain
                        "wss" => Some(443), //Tls
                        _ => None,
                    })
                    .unwrap()[0];
                (addr, url)
            }
        };

        // Synchronous tcp handshake
        let stream = StdTcpStream::connect(peer_addr)?;
        let local_addr = stream.local_addr()?;

        // Make it an asynchronous mio TcpStream
        stream.set_nonblocking(true)?;
        let stream = TcpStream::from_std(stream);

        // Synchronous waiting for web socket handshake
        let mut handshake_result = ws_connect(url, stream);
        let remote = loop {
            match handshake_result {
                Ok((ws_socket, _)) => break ws_socket.into(),
                Err(HandshakeError::Interrupted(mid_handshake)) => {
                    handshake_result = mid_handshake.handshake();
                }
                Err(HandshakeError::Failure(err)) => panic!("WS connect handshak error: {}", err),
            }
        };

        Ok(ConnectionInfo { remote, peer_addr, local_addr })
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        loop {
            match self.web_socket.lock().expect(OTHER_THREAD_ERR).read_message() {
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
                Err(err) => {
                    log::error!("WS receive error: {}", err);
                    break ReadStatus::Disconnected // should not happen
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        let message = Message::Binary(data.to_vec());
        let mut socket = self.web_socket.lock().expect(OTHER_THREAD_ERR);
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
        Ok(ListeningInfo { local: LocalResource { listener }, local_addr })
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    let mut handshake_result = ws_accept(stream);
                    let ws_socket = loop {
                        match handshake_result {
                            Ok(ws_socket) => break ws_socket.into(),
                            Err(HandshakeError::Interrupted(mid_handshake)) => {
                                handshake_result = mid_handshake.handshake();
                            }
                            Err(HandshakeError::Failure(err)) => {
                                panic!("Ws accept handshake error: {}", err)
                            }
                        }
                    };

                    accept_remote(AcceptedType::Remote(addr, ws_socket));
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("WS accept error: {}", err), // Should not happen
            }
        }
    }
}
