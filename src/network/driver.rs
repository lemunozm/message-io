use super::endpoint::{Endpoint};
use super::resource_id::{ResourceId, ResourceType};
use super::poll::{Poll, Readiness};
use super::registry::{ResourceRegistry, Register};
use super::remote_addr::{RemoteAddr};
use super::adapter::{Adapter, Remote, Local, SendStatus, AcceptedType, ReadStatus, PendingStatus};

use std::net::{SocketAddr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::io::{self};

#[cfg(doctest)]
use super::transport::{Transport};

/// Enum used to describe a network event that an internal transport adapter has produced.
pub enum NetEvent<'a> {
    /// Connection result.
    /// This event is only generated after a [`crate::network::NetworkController::connect()`]
    /// call.
    /// The event contains the endpoint of the connection
    /// (same endpoint returned by the `connect()` method),
    /// and a boolean indicating the *result* of that connection.
    /// In *non connection-oriented transports* as *UDP* it simply means that the resource
    /// is ready to use, and the boolean will be always `true`.
    /// In connection-oriented transports it means that the handshake has been performed, and the
    /// connection is established and ready to use.
    /// Since this handshake could fail, the boolean could be `false`.
    Connected(Endpoint, bool),

    /// New endpoint has been accepted by a listener and considered ready to use.
    /// The event contains the resource id of the listener that accepted this connection.
    ///
    /// Note that this event will only be generated by connection-oriented transports as *TCP*.
    Accepted(Endpoint, ResourceId),

    /// Input message received by the network.
    /// In packet-based transports, the data of a message sent corresponds with the data of this
    /// event. This one-to-one relation is not conserved in stream-based transports as *TCP*.
    ///
    /// If you want a packet-based protocol over *TCP* use
    /// [`crate::network::Transport::FramedTcp`].
    Message(Endpoint, &'a [u8]),

    /// This event is only dispatched when a connection is lost.
    /// Remove explicitely a resource will NOT generate the event.
    /// When this event is received, the resource is considered already removed,
    /// the user do not need to remove it after this event.
    /// A [`NetEvent::Message`] event will never be generated after this event from this endpoint.

    /// Note that this event will only be generated by connection-oriented transports as *TCP*.
    /// *UDP*, for example, is NOT connection-oriented, and the event can no be detected.
    Disconnected(Endpoint),
}

impl std::fmt::Debug for NetEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            Self::Connected(endpoint, status) => format!("Connected({endpoint}, {status})"),
            Self::Accepted(endpoint, id) => format!("Accepted({endpoint}, {id})"),
            Self::Message(endpoint, data) => format!("Message({}, {})", endpoint, data.len()),
            Self::Disconnected(endpoint) => format!("Disconnected({endpoint})"),
        };
        write!(f, "NetEvent::{string}")
    }
}

pub trait ActionController: Send + Sync {
    fn connect(&self, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)>;
    fn listen(&self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)>;
    fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus;
    fn remove(&self, id: ResourceId) -> bool;
    fn is_ready(&self, id: ResourceId) -> Option<bool>;
}

pub trait EventProcessor: Send + Sync {
    fn process(&self, id: ResourceId, readiness: Readiness, callback: &mut dyn FnMut(NetEvent<'_>));
}

struct RemoteProperties {
    peer_addr: SocketAddr,
    local: Option<ResourceId>,
    ready: AtomicBool,
}

impl RemoteProperties {
    fn new(peer_addr: SocketAddr, local: Option<ResourceId>) -> Self {
        Self { peer_addr, local, ready: AtomicBool::new(false) }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn mark_as_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
    }
}

struct LocalProperties;

pub struct Driver<R: Remote, L: Local> {
    remote_registry: Arc<ResourceRegistry<R, RemoteProperties>>,
    local_registry: Arc<ResourceRegistry<L, LocalProperties>>,
}

impl<R: Remote, L: Local> Driver<R, L> {
    pub fn new(
        _: impl Adapter<Remote = R, Local = L>,
        adapter_id: u8,
        poll: &mut Poll,
    ) -> Driver<R, L> {
        let remote_poll_registry = poll.create_registry(adapter_id, ResourceType::Remote);
        let local_poll_registry = poll.create_registry(adapter_id, ResourceType::Local);

        Driver {
            remote_registry: Arc::new(ResourceRegistry::<R, RemoteProperties>::new(
                remote_poll_registry,
            )),
            local_registry: Arc::new(ResourceRegistry::<L, LocalProperties>::new(
                local_poll_registry,
            )),
        }
    }
}

impl<R: Remote, L: Local> Clone for Driver<R, L> {
    fn clone(&self) -> Driver<R, L> {
        Driver {
            remote_registry: self.remote_registry.clone(),
            local_registry: self.local_registry.clone(),
        }
    }
}

impl<R: Remote, L: Local> ActionController for Driver<R, L> {
    fn connect(&self, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        R::connect(addr).map(|info| {
            let id = self.remote_registry.register(
                info.remote,
                RemoteProperties::new(info.peer_addr, None),
                true,
            );
            (Endpoint::new(id, info.peer_addr), info.local_addr)
        })
    }

    fn listen(&self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        L::listen(addr).map(|info| {
            let id = self.local_registry.register(info.local, LocalProperties, false);
            (id, info.local_addr)
        })
    }

    fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        match endpoint.resource_id().resource_type() {
            ResourceType::Remote => match self.remote_registry.get(endpoint.resource_id()) {
                Some(remote) => match remote.properties.is_ready() {
                    true => remote.resource.send(data),
                    false => SendStatus::ResourceNotAvailable,
                },
                None => SendStatus::ResourceNotFound,
            },
            ResourceType::Local => match self.local_registry.get(endpoint.resource_id()) {
                Some(remote) => remote.resource.send_to(endpoint.addr(), data),
                None => SendStatus::ResourceNotFound,
            },
        }
    }

    fn remove(&self, id: ResourceId) -> bool {
        match id.resource_type() {
            ResourceType::Remote => self.remote_registry.deregister(id),
            ResourceType::Local => self.local_registry.deregister(id),
        }
    }

    fn is_ready(&self, id: ResourceId) -> Option<bool> {
        match id.resource_type() {
            ResourceType::Remote => self.remote_registry.get(id).map(|r| r.properties.is_ready()),
            ResourceType::Local => self.local_registry.get(id).map(|_| true),
        }
    }
}

impl<R: Remote, L: Local<Remote = R>> EventProcessor for Driver<R, L> {
    fn process(
        &self,
        id: ResourceId,
        readiness: Readiness,
        event_callback: &mut dyn FnMut(NetEvent<'_>),
    ) {
        match id.resource_type() {
            ResourceType::Remote => {
                if let Some(remote) = self.remote_registry.get(id) {
                    let endpoint = Endpoint::new(id, remote.properties.peer_addr);
                    log::trace!("Processed remote for {}", endpoint);

                    if !remote.properties.is_ready() {
                        self.resolve_pending_remote(&remote, endpoint, readiness, |e| {
                            event_callback(e)
                        });
                    }
                    if remote.properties.is_ready() {
                        match readiness {
                            Readiness::Write => {
                                self.write_to_remote(&remote, endpoint, event_callback);
                            }
                            Readiness::Read => {
                                self.read_from_remote(&remote, endpoint, event_callback);
                            }
                        }
                    }
                }
            }
            ResourceType::Local => {
                if let Some(local) = self.local_registry.get(id) {
                    log::trace!("Processed local for {}", id);
                    match readiness {
                        Readiness::Write => (),
                        Readiness::Read => self.read_from_local(&local, id, event_callback),
                    }
                }
            }
        }
    }
}

impl<R: Remote, L: Local<Remote = R>> Driver<R, L> {
    fn resolve_pending_remote(
        &self,
        remote: &Arc<Register<R, RemoteProperties>>,
        endpoint: Endpoint,
        readiness: Readiness,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        let status = remote.resource.pending(readiness);
        log::trace!("Resolve pending for {}: {:?}", endpoint, status);
        match status {
            PendingStatus::Ready => {
                remote.properties.mark_as_ready();
                match remote.properties.local {
                    Some(listener_id) => event_callback(NetEvent::Accepted(endpoint, listener_id)),
                    None => event_callback(NetEvent::Connected(endpoint, true)),
                }
                remote.resource.ready_to_write();
            }
            PendingStatus::Incomplete => (),
            PendingStatus::Disconnected => {
                self.remote_registry.deregister(endpoint.resource_id());
                if remote.properties.local.is_none() {
                    event_callback(NetEvent::Connected(endpoint, false));
                }
            }
        }
    }

    fn write_to_remote(
        &self,
        remote: &Arc<Register<R, RemoteProperties>>,
        endpoint: Endpoint,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        if !remote.resource.ready_to_write() {
            event_callback(NetEvent::Disconnected(endpoint));
        }
    }

    fn read_from_remote(
        &self,
        remote: &Arc<Register<R, RemoteProperties>>,
        endpoint: Endpoint,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        let status =
            remote.resource.receive(|data| event_callback(NetEvent::Message(endpoint, data)));
        log::trace!("Receive status: {:?}", status);
        if let ReadStatus::Disconnected = status {
            // Checked because, the user in the callback could have removed the same resource.
            if self.remote_registry.deregister(endpoint.resource_id()) {
                event_callback(NetEvent::Disconnected(endpoint));
            }
        }
    }

    fn read_from_local(
        &self,
        local: &Arc<Register<L, LocalProperties>>,
        id: ResourceId,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        local.resource.accept(|accepted| {
            log::trace!("Accepted type: {}", accepted);
            match accepted {
                AcceptedType::Remote(addr, remote) => {
                    self.remote_registry.register(
                        remote,
                        RemoteProperties::new(addr, Some(id)),
                        true,
                    );
                }
                AcceptedType::Data(addr, data) => {
                    let endpoint = Endpoint::new(id, addr);
                    event_callback(NetEvent::Message(endpoint, data));
                }
            }
        });
    }
}

impl<R> std::fmt::Display for AcceptedType<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            AcceptedType::Remote(addr, _) => format!("Remote({addr})"),
            AcceptedType::Data(addr, _) => format!("Data({addr})"),
        };
        write!(f, "AcceptedType::{string}")
    }
}
