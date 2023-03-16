use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness};
use crate::util::thread::{OTHER_THREAD_ERR};
use crate::network::{TransportConnect, TransportListen};

use mio::event::{Source};
use mio::net::{TcpStream, TcpListener};

use tungstenite::protocol::{WebSocket, Message};
use tungstenite::{accept as ws_accept};
use tungstenite::client::{client as ws_connect};
use tungstenite::handshake::{
    HandshakeError, MidHandshake,
    server::{ServerHandshake, NoCallback},
    client::{ClientHandshake},
};
use tungstenite::error::{Error};

use url::Url;

use std::sync::{Mutex, Arc};
use std::net::{SocketAddr};
use std::io::{self, ErrorKind};
use std::ops::{DerefMut};

/// Max message size for default config
// From https://docs.rs/tungstenite/0.13.0/src/tungstenite/protocol/mod.rs.html#65
pub const MAX_PAYLOAD_LEN: usize = 32 << 20;

pub(crate) struct WsAdapter;
impl Adapter for WsAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

enum PendingHandshake {
    Connect(Url, ArcTcpStream),
    Accept(ArcTcpStream),
    Client(MidHandshake<ClientHandshake<ArcTcpStream>>),
    Server(MidHandshake<ServerHandshake<ArcTcpStream, NoCallback>>),
}

#[allow(clippy::large_enum_variant)]
enum RemoteState {
    WebSocket(WebSocket<ArcTcpStream>),
    Handshake(Option<PendingHandshake>),
    Error(ArcTcpStream),
}

pub(crate) struct RemoteResource {
    state: Mutex<RemoteState>,
}

impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        match self.state.get_mut().unwrap() {
            RemoteState::WebSocket(web_socket) => {
                Arc::get_mut(&mut web_socket.get_mut().0).unwrap()
            }
            RemoteState::Handshake(Some(handshake)) => match handshake {
                PendingHandshake::Connect(_, stream) => Arc::get_mut(&mut stream.0).unwrap(),
                PendingHandshake::Accept(stream) => Arc::get_mut(&mut stream.0).unwrap(),
                PendingHandshake::Client(handshake) => {
                    Arc::get_mut(&mut handshake.get_mut().get_mut().0).unwrap()
                }
                PendingHandshake::Server(handshake) => {
                    Arc::get_mut(&mut handshake.get_mut().get_mut().0).unwrap()
                }
            },
            RemoteState::Handshake(None) => unreachable!(),
            RemoteState::Error(stream) => Arc::get_mut(&mut stream.0).unwrap(),
        }
    }
}

impl Remote for RemoteResource {
    fn connect_with(
        _: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        let (peer_addr, url) = match remote_addr {
            RemoteAddr::Socket(addr) => {
                (addr, Url::parse(&format!("ws://{addr}/message-io-default")).unwrap())
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

        let stream = TcpStream::connect(peer_addr)?;
        let local_addr = stream.local_addr()?;

        Ok(ConnectionInfo {
            remote: RemoteResource {
                state: Mutex::new(RemoteState::Handshake(Some(PendingHandshake::Connect(
                    url,
                    stream.into(),
                )))),
            },
            local_addr,
            peer_addr,
        })
    }

    fn receive(&self, mut process_data: impl FnMut(&[u8])) -> ReadStatus {
        loop {
            // "emulates" full duplex for the websocket case locking here and not outside the loop.
            let mut state = self.state.lock().expect(OTHER_THREAD_ERR);
            let deref_state = state.deref_mut();
            match deref_state {
                RemoteState::WebSocket(web_socket) => match web_socket.read_message() {
                    Ok(message) => match message {
                        Message::Binary(data) => {
                            // As an optimization.
                            // Fast check to know if there is more data to avoid call
                            // WebSocket::read_message() again.
                            // TODO: investigate why this code doesn't work in windows.
                            // Seems like windows consume the `WouldBlock` notification
                            // at peek() when it happens, and the poll never wakes it again.
                            #[cfg(not(target_os = "windows"))]
                            let _peek_result = web_socket.get_ref().0.peek(&mut [0; 0]);

                            // We can not call process_data while the socket is blocked.
                            // The user could lock it again if sends from the callback.
                            drop(state);
                            process_data(&data);

                            #[cfg(not(target_os = "windows"))]
                            if let Err(err) = _peek_result {
                                break Self::io_error_to_read_status(&err)
                            }
                        }
                        Message::Close(_) => break ReadStatus::Disconnected,
                        _ => continue,
                    },
                    Err(Error::Io(ref err)) => break Self::io_error_to_read_status(err),
                    Err(err) => {
                        log::error!("WS receive error: {}", err);
                        break ReadStatus::Disconnected // should not happen
                    }
                },
                RemoteState::Handshake(_) => unreachable!(),
                RemoteState::Error(_) => unreachable!(),
            }
        }
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        let mut state = self.state.lock().expect(OTHER_THREAD_ERR);
        let deref_state = state.deref_mut();
        match deref_state {
            RemoteState::WebSocket(web_socket) => {
                let message = Message::Binary(data.to_vec());
                let mut result = web_socket.write_message(message);
                loop {
                    match result {
                        Ok(_) => break SendStatus::Sent,
                        Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock => {
                            result = web_socket.write_pending();
                        }
                        Err(Error::Capacity(_)) => break SendStatus::MaxPacketSizeExceeded,
                        Err(err) => {
                            log::error!("WS send error: {}", err);
                            break SendStatus::ResourceNotFound // should not happen
                        }
                    }
                }
            }
            RemoteState::Handshake(_) => unreachable!(),
            RemoteState::Error(_) => unreachable!(),
        }
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        let mut state = self.state.lock().expect(OTHER_THREAD_ERR);
        let deref_state = state.deref_mut();
        match deref_state {
            RemoteState::WebSocket(_) => PendingStatus::Ready,
            RemoteState::Handshake(pending) => match pending.take().unwrap() {
                PendingHandshake::Connect(url, stream) => {
                    let tcp_status = super::tcp::check_stream_ready(&stream.0);
                    if tcp_status != PendingStatus::Ready {
                        // TCP handshake not ready yet.
                        *pending = Some(PendingHandshake::Connect(url, stream));
                        return tcp_status
                    }
                    let stream_backup = stream.clone();
                    match ws_connect(url, stream) {
                        Ok((web_socket, _)) => {
                            *state = RemoteState::WebSocket(web_socket);
                            PendingStatus::Ready
                        }
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            *pending = Some(PendingHandshake::Client(mid_handshake));
                            PendingStatus::Incomplete
                        }
                        Err(HandshakeError::Failure(Error::Io(_))) => {
                            *state = RemoteState::Error(stream_backup);
                            PendingStatus::Disconnected
                        }
                        Err(HandshakeError::Failure(err)) => {
                            *state = RemoteState::Error(stream_backup);
                            log::error!("WS connect handshake error: {}", err);
                            PendingStatus::Disconnected // should not happen
                        }
                    }
                }
                PendingHandshake::Accept(stream) => {
                    let stream_backup = stream.clone();
                    match ws_accept(stream) {
                        Ok(web_socket) => {
                            *state = RemoteState::WebSocket(web_socket);
                            PendingStatus::Ready
                        }
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            *pending = Some(PendingHandshake::Server(mid_handshake));
                            PendingStatus::Incomplete
                        }
                        Err(HandshakeError::Failure(Error::Io(_))) => {
                            *state = RemoteState::Error(stream_backup);
                            PendingStatus::Disconnected
                        }
                        Err(HandshakeError::Failure(err)) => {
                            *state = RemoteState::Error(stream_backup);
                            log::error!("WS accept handshake error: {}", err);
                            PendingStatus::Disconnected
                        }
                    }
                }
                PendingHandshake::Client(mid_handshake) => {
                    let stream_backup = mid_handshake.get_ref().get_ref().clone();
                    match mid_handshake.handshake() {
                        Ok((web_socket, _)) => {
                            *state = RemoteState::WebSocket(web_socket);
                            PendingStatus::Ready
                        }
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            *pending = Some(PendingHandshake::Client(mid_handshake));
                            PendingStatus::Incomplete
                        }
                        Err(HandshakeError::Failure(Error::Io(_))) => {
                            *state = RemoteState::Error(stream_backup);
                            PendingStatus::Disconnected
                        }
                        Err(HandshakeError::Failure(err)) => {
                            *state = RemoteState::Error(stream_backup);
                            log::error!("WS client handshake error: {}", err);
                            PendingStatus::Disconnected // should not happen
                        }
                    }
                }
                PendingHandshake::Server(mid_handshake) => {
                    let stream_backup = mid_handshake.get_ref().get_ref().clone();
                    match mid_handshake.handshake() {
                        Ok(web_socket) => {
                            *state = RemoteState::WebSocket(web_socket);
                            PendingStatus::Ready
                        }
                        Err(HandshakeError::Interrupted(mid_handshake)) => {
                            *pending = Some(PendingHandshake::Server(mid_handshake));
                            PendingStatus::Incomplete
                        }
                        Err(HandshakeError::Failure(Error::Io(_))) => {
                            *state = RemoteState::Error(stream_backup);
                            PendingStatus::Disconnected
                        }
                        Err(HandshakeError::Failure(err)) => {
                            *state = RemoteState::Error(stream_backup);
                            log::error!("WS server handshake error: {}", err);
                            PendingStatus::Disconnected // should not happen
                        }
                    }
                }
            },
            RemoteState::Error(_) => unreachable!(),
        }
    }

    fn ready_to_write(&self) -> bool {
        true
        /* Is this needed?
        match self.state.lock().expect(OTHER_THREAD_ERR).deref_mut() {
            RemoteState::WebSocket(web_socket) => match web_socket.write_pending() {
                Ok(_) => true,
                Err(Error::Io(ref err)) if err.kind() == ErrorKind::WouldBlock => true,
                Err(_) => false, // Will be disconnected,
            },
            // This function is only call on ready resources.
            RemoteState::Handshake(_) => unreachable!(),
            RemoteState::Error(_) => unreachable!(),
        }
        */
    }
}

impl RemoteResource {
    fn io_error_to_read_status(err: &io::Error) -> ReadStatus {
        if err.kind() == io::ErrorKind::WouldBlock {
            ReadStatus::WaitNextEvent
        }
        else if err.kind() == io::ErrorKind::ConnectionReset {
            ReadStatus::Disconnected
        }
        else {
            log::error!("WS receive error: {}", err);
            ReadStatus::Disconnected // should not happen
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

    fn listen_with(_: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        let listener = TcpListener::bind(addr)?;
        let local_addr = listener.local_addr().unwrap();
        Ok(ListeningInfo { local: LocalResource { listener }, local_addr })
    }

    fn accept(&self, mut accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    let remote = RemoteResource {
                        state: Mutex::new(RemoteState::Handshake(Some(PendingHandshake::Accept(
                            stream.into(),
                        )))),
                    };
                    accept_remote(AcceptedType::Remote(addr, remote));
                }
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => break log::error!("WS accept error: {}", err), // Should not happen
            }
        }
    }
}

/// This struct is used to avoid the tungstenite handshake to take the ownership of the stream
/// an drop it without allow to the driver to deregister from the poll.
/// It can be removed when this issue is resolved:
/// https://github.com/snapview/tungstenite-rs/issues/51
struct ArcTcpStream(Arc<TcpStream>);

impl From<TcpStream> for ArcTcpStream {
    fn from(stream: TcpStream) -> Self {
        Self(Arc::new(stream))
    }
}

impl io::Read for ArcTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl io::Write for ArcTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

impl Clone for ArcTcpStream {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
