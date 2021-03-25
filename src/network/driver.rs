use super::endpoint::{Endpoint};
use super::resource_id::{ResourceId, ResourceType};
use super::poll::{Poll};
use super::registry::{ResourceRegistry};
use super::remote_addr::{RemoteAddr};

use crate::adapter::{Adapter, Remote, Local, SendStatus, AcceptedType, ReadStatus};

use std::net::{SocketAddr};
use std::sync::{Arc};
use std::io::{self};

/// Enum used to describe and event that an adapter network has produced.
#[derive(Debug)]
pub enum NetEvent<'a> {
    /// New endpoint has been connected to a listener.
    /// This event will be sent only in connection oriented protocols as [`Transport::Tcp`].
    /// It also contains the resource id of the listener that accepted this connection.
    Connected(Endpoint, ResourceId),

    /// Input message received by the network.
    Message(Endpoint, &'a [u8]),

    /// This event is only dispatched when a connection is lost.
    /// Remove explicitely a resource will NOT generate the event.
    /// When this event is received, the resource is considered already removed,
    /// the user do not need to remove it after this event.
    /// A [`NetEvent::Message`] event will never be generated after this event from this endpoint.
    /// This event will be sent only in connection oriented protocols as *Tcp*.
    /// *UDP*, for example, is NOT connection oriented, and the event can no be detected.
    Disconnected(Endpoint),
}

pub trait ActionController: Send + Sync {
    fn connect(&self, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)>;
    fn listen(&self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)>;
    fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus;
    fn remove(&self, id: ResourceId) -> bool;
}

pub trait EventProcessor: Send + Sync {
    fn process(&self, resource_id: ResourceId, event_callback: &dyn Fn(NetEvent<'_>));
}

pub struct Driver<R: Remote, L: Local> {
    remote_registry: Arc<ResourceRegistry<R>>,
    local_registry: Arc<ResourceRegistry<L>>,
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
            remote_registry: Arc::new(ResourceRegistry::<R>::new(remote_poll_registry)),
            local_registry: Arc::new(ResourceRegistry::<L>::new(local_poll_registry)),
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
            (
                Endpoint::new(
                    self.remote_registry.add(info.remote, info.peer_addr),
                    info.peer_addr,
                ),
                info.local_addr,
            )
        })
    }

    fn listen(&self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        L::listen(addr)
            .map(|info| (self.local_registry.add(info.local, info.local_addr), info.local_addr))
    }

    fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        match endpoint.resource_id().resource_type() {
            ResourceType::Remote => {
                match self.remote_registry.get(endpoint.resource_id()) {
                    Some(remote) => remote.resource.send(data),

                    // TODO: currently there is not a safe way to know if this is
                    // reached because of a user API error (send over already resource removed)
                    // or because of a disconnection happened but not processed yet.
                    // It could be better to panics in the first case to distinguish
                    // the programming error from the second case.
                    None => SendStatus::ResourceNotFound,
                }
            }
            ResourceType::Local => match self.local_registry.get(endpoint.resource_id()) {
                Some(remote) => remote.resource.send_to(endpoint.addr(), data),
                None => {
                    panic!(
                        "Error: You are trying to send by a local resource \
                        that does not exists"
                    )
                }
            },
        }
    }

    fn remove(&self, id: ResourceId) -> bool {
        match id.resource_type() {
            ResourceType::Remote => self.remote_registry.remove(id),
            ResourceType::Local => self.local_registry.remove(id),
        }
    }
}

impl<R: Remote, L: Local<Remote = R>> EventProcessor for Driver<R, L> {
    fn process(&self, id: ResourceId, event_callback: &dyn Fn(NetEvent<'_>)) {
        match id.resource_type() {
            ResourceType::Remote => self.process_remote(id, event_callback),
            ResourceType::Local => self.process_local(id, event_callback),
        }
    }
}

impl<R: Remote, L: Local<Remote = R>> Driver<R, L> {
    fn process_remote(&self, id: ResourceId, event_callback: &dyn Fn(NetEvent<'_>)) {
        if let Some(remote) = self.remote_registry.get(id) {
            let endpoint = Endpoint::new(id, remote.addr);
            log::trace!("Processed remote for {}", endpoint);
            let status = remote.resource.receive(&|data| {
                event_callback(NetEvent::Message(endpoint, data));
            });
            log::trace!("Processed remote receive status {}", status);

            if let ReadStatus::Disconnected = status {
                // Checked becasue, the user in the callback could have removed the same resource.
                if self.remote_registry.remove(id) {
                    event_callback(NetEvent::Disconnected(endpoint));
                }
            }
        }
    }

    fn process_local(&self, id: ResourceId, event_callback: &dyn Fn(NetEvent<'_>)) {
        if let Some(local) = self.local_registry.get(id) {
            log::trace!("Processed local for {}", id);
            local.resource.accept(&|accepted| {
                log::trace!("Processed local accepted type {}", accepted);
                match accepted {
                    AcceptedType::Remote(addr, remote) => {
                        let remote_id = self.remote_registry.add(remote, addr);
                        let endpoint = Endpoint::new(remote_id, addr);
                        event_callback(NetEvent::Connected(endpoint, id));
                    }
                    AcceptedType::Data(addr, data) => {
                        let endpoint = Endpoint::new(id, addr);
                        event_callback(NetEvent::Message(endpoint, data));
                    }
                }
            });
        }
    }
}

impl std::fmt::Display for ReadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            ReadStatus::Disconnected => "Disconnected",
            ReadStatus::WaitNextEvent => "WaitNextEvent",
        };
        write!(f, "ReadStatus::{}", string)
    }
}

impl<R> std::fmt::Display for AcceptedType<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            AcceptedType::Remote(addr, _) => format!("Remote({})", addr),
            AcceptedType::Data(addr, _) => format!("Data({})", addr),
        };
        write!(f, "AcceptedType::{}", string)
    }
}
