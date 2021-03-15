use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::poll::{Poll, PollRegister};
use crate::remote_addr::{RemoteAddr};
use crate::adapter::{Resource, Adapter, Remote, Local, SendStatus, AcceptedType, ReadStatus};
use crate::util::{OTHER_THREAD_ERR};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{Arc, RwLock};
use std::io::{self};

/// Struct used to identify and event that an adapter has been produced.
/// The upper layer can traduce this event into a [crate::network::NetEvent]
/// that the user can manage easily.
#[derive(Debug)]
pub enum AdapterEvent<'a> {
    /// The endpoint has been added (it implies a connection).
    /// It also contains the resource id of the listener that accepted this endpoint.
    Added(Endpoint, ResourceId),

    /// The endpoint has sent data that represents a message.
    Data(Endpoint, &'a [u8]),

    /// The endpoint has been removed (it implies a disconnection).
    Removed(Endpoint),
}

pub struct ResourceRegister<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, (S, SocketAddr)>>,
    poll_register: PollRegister,
}

impl<S: Resource> ResourceRegister<S> {
    pub fn new(poll_register: PollRegister) -> ResourceRegister<S> {
        ResourceRegister { resources: RwLock::new(HashMap::new()), poll_register }
    }

    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_register.add(resource.source());
        self.resources.write().expect(OTHER_THREAD_ERR).insert(id, (resource, addr));
        id
    }

    pub fn remove(&self, id: ResourceId) -> Option<(S, SocketAddr)> {
        let poll_register = &self.poll_register;
        self.resources.write().expect(OTHER_THREAD_ERR).remove(&id).map(|(mut resource, addr)| {
            poll_register.remove(resource.source());
            (resource, addr)
        })
    }

    pub fn resources(&self) -> &RwLock<HashMap<ResourceId, (S, SocketAddr)>> {
        &self.resources
    }
}

pub trait ActionController {
    fn connect(&mut self, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)>;
    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)>;
    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus;
    fn remove(&mut self, id: ResourceId) -> bool;
}

pub trait EventProcessor {
    fn try_process(&mut self, id: ResourceId, event_callback: &dyn Fn(AdapterEvent<'_>));
}

pub struct Driver<R: Remote, L: Local> {
    remote_register: Arc<ResourceRegister<R>>,
    local_register: Arc<ResourceRegister<L>>,
}

impl<R: Remote, L: Local> Driver<R, L> {
    pub fn new(
        _: impl Adapter<Remote = R, Local = L>,
        adapter_id: u8,
        poll: &mut Poll,
    ) -> Driver<R, L> {
        let remote_poll_register = poll.create_register(adapter_id, ResourceType::Remote);
        let local_poll_register = poll.create_register(adapter_id, ResourceType::Local);

        Driver {
            remote_register: Arc::new(ResourceRegister::<R>::new(remote_poll_register)),
            local_register: Arc::new(ResourceRegister::<L>::new(local_poll_register)),
        }
    }
}

impl<R: Remote, L: Local> Clone for Driver<R, L> {
    fn clone(&self) -> Driver<R, L> {
        Driver {
            remote_register: self.remote_register.clone(),
            local_register: self.local_register.clone(),
        }
    }
}

impl<R: Remote, L: Local> ActionController for Driver<R, L> {
    fn connect(&mut self, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        let remotes = &mut self.remote_register;
        R::connect(addr).map(|info| {
            (
                Endpoint::new(remotes.add(info.remote, info.peer_addr), info.peer_addr),
                info.local_addr,
            )
        })
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let locals = &mut self.local_register;
        L::listen(addr).map(|info| (locals.add(info.local, info.local_addr), info.local_addr))
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        match endpoint.resource_id().resource_type() {
            ResourceType::Remote => {
                let remotes = self.remote_register.resources().read().expect(OTHER_THREAD_ERR);
                match remotes.get(&endpoint.resource_id()) {
                    Some((resource, _)) => resource.send(data),

                    // TODO: currently there is not a safe way to know if this is
                    // reached because of a user API error (send over already resource removed)
                    // or because of a disconnection happened but not processed yet.
                    // It could be better to panics in the first case to distinguish
                    // the programming error from the second case.
                    None => SendStatus::ResourceNotFound,
                }
            }
            ResourceType::Local => {
                let locals = self.local_register.resources().read().expect(OTHER_THREAD_ERR);
                match locals.get(&endpoint.resource_id()) {
                    Some((resource, _)) => resource.send_to(endpoint.addr(), data),
                    None => {
                        panic!(
                            "Error: You are trying to send by a local resource \
                               that does not exists"
                        )
                    }
                }
            }
        }
    }

    fn remove(&mut self, id: ResourceId) -> bool {
        match id.resource_type() {
            ResourceType::Remote => self.remote_register.remove(id).map(|_| ()).is_some(),
            ResourceType::Local => self.local_register.remove(id).map(|_| ()).is_some(),
        }
    }
}

impl<R: Remote, L: Local<Remote = R>> EventProcessor for Driver<R, L> {
    fn try_process(&mut self, id: ResourceId, event_callback: &dyn Fn(AdapterEvent<'_>)) {
        match id.resource_type() {
            ResourceType::Remote => {
                let remotes = self.remote_register.resources().read().expect(OTHER_THREAD_ERR);
                let mut to_remove: Option<Endpoint> = None;
                if let Some((remote, addr)) = remotes.get(&id) {
                    let endpoint = Endpoint::new(id, *addr);
                    let status = remote.receive(&|data| {
                        log::trace!("Read {} bytes from {}", data.len(), id);
                        event_callback(AdapterEvent::Data(endpoint, data));
                    });
                    log::trace!("Processed receive {}, for {}", status, endpoint);
                    if let ReadStatus::Disconnected = status {
                        to_remove = Some(endpoint);
                    }
                }

                drop(remotes);

                if let Some(endpoint) = to_remove {
                    self.remote_register.remove(id);
                    event_callback(AdapterEvent::Removed(endpoint));
                }
            }
            ResourceType::Local => {
                let locals = self.local_register.resources().read().expect(OTHER_THREAD_ERR);

                let remotes = &mut self.remote_register;

                if let Some((local, _)) = locals.get(&id) {
                    local.accept(&|accepted| {
                        log::trace!("Processed accept {} for {}", accepted, id);
                        match accepted {
                            AcceptedType::Remote(addr, remote) => {
                                let remote_id = remotes.add(remote, addr);
                                let endpoint = Endpoint::new(remote_id, addr);
                                event_callback(AdapterEvent::Added(endpoint, id));
                            }
                            AcceptedType::Data(addr, data) => {
                                let endpoint = Endpoint::new(id, addr);
                                event_callback(AdapterEvent::Data(endpoint, data));
                            }
                        }
                    });
                }
            }
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
