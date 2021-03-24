use crate::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo,
};
use crate::network::{RemoteAddr};
use crate::util::thread::{OTHER_THREAD_ERR};

use mio::event::{Source};
use mio::net::{TcpStream, TcpListener};

use tungstenite::protocol::{WebSocket, Message};
use tungstenite::server::{accept as ws_accept};
use tungstenite::client::{client as ws_connect};
use tungstenite::handshake::{
    HandshakeError, MidHandshake,
    server::{ServerHandshake, NoCallback},
};
use tungstenite::error::{Error};

use url::Url;

use std::sync::{Mutex};
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::io::{self, ErrorKind};
use std::ops::{DerefMut};

/// Max message size for default config
// From https://docs.rs/tungstenite/0.13.0/src/tungstenite/protocol/mod.rs.html#65
pub const MAX_WS_PAYLOAD_LEN: usize = 32 << 20;

pub struct WsAdapter;
impl Adapter for WsAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

struct PendingHandshake {
    mid_handshake: MidHandshake<ServerHandshake<TcpStream, NoCallback>>,
    pending_messages: Vec<Vec<u8>>,
}

enum RemoteState {
    WebSocket(WebSocket<TcpStream>),
    Handshake(Option<PendingHandshake>),
}

pub struct RemoteResource {
    state: Mutex<RemoteState>,
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        match self.state.get_mut().unwrap() {
            RemoteState::WebSocket(web_socket) => web_socket.get_mut(),
            RemoteState::Handshake(Some(handshake)) => handshake.mid_handshake.get_mut().get_mut(),
            RemoteState::Handshake(None) => unreachable!(),
        }
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<ConnectionInfo<Self>> {
        let (peer_addr, url) = match remote_addr {
            RemoteAddr::Socket(addr) => {
                (addr, Url::parse(&format!("ws://{}/message-io-default", addr)).unwrap())
            }
            RemoteAddr::Str(path) => {
                let url = Url::parse(&path).expect("A valid URL");
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
                Ok((web_socket, _)) => {
                    break RemoteResource { state: Mutex::new(RemoteState::WebSocket(web_socket)) }
                }
                Err(HandshakeError::Interrupted(mid_handshake)) => {
                    handshake_result = mid_handshake.handshake();
                }
                Err(HandshakeError::Failure(err)) => {
                    //CHECK: give to the user an io::Error?
                    panic!("WS connect handshake error: {}", err)
                }
            }
        };

        Ok(ConnectionInfo { remote, local_addr, peer_addr })
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        loop {
            // It is preferred to lock inside the loop to avoid blocking the sender thread
            // if there is a huge amount of data to read.
            // This way we "emulates" full duplex for the websocket case.
            let mut state = self.state.lock().expect(OTHER_THREAD_ERR);
            match state.deref_mut() {
                RemoteState::WebSocket(web_socket) => {
                    match web_socket.read_message() {
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
                RemoteState::Handshake(handshake) => {
                    let current_handshake = handshake.take().unwrap();
                    match current_handshake.mid_handshake.handshake() {
                        Ok(mut web_socket) => {
                            for pending_data in current_handshake.pending_messages {
                                Self::send_by_socket(&mut web_socket, &pending_data);
                            }
                            *state = RemoteState::WebSocket(web_socket);
                        }
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            *handshake = Some(PendingHandshake {
                                mid_handshake,
                                pending_messages: current_handshake.pending_messages,
                            });
                            break ReadStatus::WaitNextEvent
                        }
                        Err(HandshakeError::Failure(ref err)) => {
                            log::error!("WS accept handshake error: {}", err);
                            break ReadStatus::Disconnected // should not happen
                        }
                    }
                }
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        match self.state.lock().expect(OTHER_THREAD_ERR).deref_mut() {
            RemoteState::WebSocket(web_socket) => Self::send_by_socket(web_socket, data),
            RemoteState::Handshake(handshake) => {
                handshake.as_mut().unwrap().pending_messages.push(data.to_vec());
                SendStatus::Sent //Future versions: SendStatus::Enqueued
            }
        }
    }
}

impl RemoteResource {
    fn send_by_socket(web_socket: &mut WebSocket<TcpStream>, data: &[u8]) -> SendStatus {
        let message = Message::Binary(data.to_vec());
        let mut result = web_socket.write_message(message);
        loop {
            match result {
                Ok(_) => break SendStatus::Sent,
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock => {
                    result = web_socket.write_pending();
                }
                Err(err) => {
                    log::error!("WS send error: {}", err);
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
        Ok(ListeningInfo { local: LocalResource { listener }, local_addr })
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    let remote_state = match ws_accept(stream) {
                        Ok(web_socket) => Some(RemoteState::WebSocket(web_socket)),
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            Some(RemoteState::Handshake(Some(PendingHandshake {
                                mid_handshake,
                                pending_messages: Vec::new(),
                            })))
                        }
                        Err(HandshakeError::Failure(ref err)) => {
                            log::error!("WS accept handshake error: {}", err);
                            None
                        }
                    };

                    if let Some(remote_state) = remote_state {
                        let remote = RemoteResource { state: Mutex::new(remote_state) };
                        accept_remote(AcceptedType::Remote(addr, remote));
                    }
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("WS accept error: {}", err), // Should not happen
            }
        }
    }
}
