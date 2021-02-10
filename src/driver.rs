use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::poll::{PollRegister};
use crate::adapter::{ActionHandler, EventHandler};
use crate::status::{SendStatus, AcceptStatus, ReadStatus};
use crate::util::{OTHER_THREAD_ERR};

use mio::event::{Source};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{Arc, RwLock};
use std::io::{self};

/// Struct used to pack the events generated by the adapter.
/// The upper layer will traduce this event into a [crate::network::NetEvent]
/// that the user can manage.
pub enum AdapterEvent<'a> {
    Added,
    Data(&'a [u8]),
    Removed,
}

pub struct ResourceRegister<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a listen resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, (S, SocketAddr)>>,
    poll_register: PollRegister,
}

impl<S: Source> ResourceRegister<S> {
    pub fn new(poll_register: PollRegister) -> ResourceRegister<S> {
        ResourceRegister { resources: RwLock::new(HashMap::new()), poll_register }
    }

    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_register.add(&mut resource);
        self.resources.write().expect(OTHER_THREAD_ERR).insert(id, (resource, addr));
        id
    }

    pub fn remove(&self, id: ResourceId) -> Option<(S, SocketAddr)> {
        let poll_register = &self.poll_register;
        self.resources.write().expect(OTHER_THREAD_ERR).remove(&id).map(|(mut resource, addr)| {
            poll_register.remove(&mut resource);
            (resource, addr)
        })
    }

    pub fn resources(&self) -> &RwLock<HashMap<ResourceId, (S, SocketAddr)>> {
        &self.resources
    }
}

pub trait ActionController {
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint>;
    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)>;
    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus;
    fn remove(&mut self, id: ResourceId) -> Option<()>;
}

pub struct GenericActionController<R: Source, L: Source> {
    remote_register: Arc<ResourceRegister<R>>,
    listener_register: Arc<ResourceRegister<L>>,
    action_handler: Box<dyn ActionHandler<Remote = R, Listener = L>>,
}

impl<R: Source, L: Source> GenericActionController<R, L> {
    pub fn new(
        remote_register: Arc<ResourceRegister<R>>,
        listener_register: Arc<ResourceRegister<L>>,
        action_handler: impl ActionHandler<Remote = R, Listener = L> + 'static,
    ) -> GenericActionController<R, L>
    {
        GenericActionController {
            remote_register,
            listener_register,
            action_handler: Box::new(action_handler),
        }
    }
}

impl<R: Source, L: Source> ActionController for GenericActionController<R, L> {
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint> {
        let remotes = &mut self.remote_register;
        self.action_handler
            .connect(addr)
            .map(|resource| remotes.add(resource, addr))
            .map(|resource_id| Endpoint::new(resource_id, addr))
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        let listeners = &mut self.listener_register;
        self.action_handler
            .listen(addr)
            .map(|(resource, addr)| (listeners.add(resource, addr), addr))
            .map(|(resource_id, real_addr)| (resource_id, real_addr))
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        match endpoint.resource_id().resource_type() {
            ResourceType::Remote => {
                let remotes = self.remote_register.resources().read().expect(OTHER_THREAD_ERR);
                match remotes.get(&endpoint.resource_id()) {
                    Some((resource, _)) => self.action_handler.send(resource, data),

                    // TODO: currently there is not a safe way to know if this is
                    // reached because of a user API error (send over already removed endpoint)
                    // or because of a disconnection happened but not processed yet.
                    // It could be better to panics in the first case to distinguish
                    // the programming error from the second case.
                    None => SendStatus::RemovedEndpoint,
                }
            }
            ResourceType::Listener => {
                let listeners = self.listener_register.resources().read().expect(OTHER_THREAD_ERR);
                match listeners.get(&endpoint.resource_id()) {
                    Some((resource, _)) => {
                        self.action_handler.send_by_listener(resource, endpoint.addr(), data)
                    }
                    None => {
                        panic!("Error: You are trying to send by a listener that does not exists")
                    }
                }
            }
        }
    }

    fn remove(&mut self, id: ResourceId) -> Option<()> {
        let action_handler = &mut self.action_handler;
        match id.resource_type() {
            ResourceType::Remote => self
                .remote_register
                .remove(id)
                .map(|(resource, addr)| action_handler.remove_remote(resource, addr)),
            ResourceType::Listener => self
                .listener_register
                .remove(id)
                .map(|(resource, addr)| action_handler.remove_listener(resource, addr)),
        }
    }
}

impl<R: Source, L: Source> Drop for GenericActionController<R, L> {
    fn drop(&mut self) {
        let remotes = self.remote_register.resources().read().expect(OTHER_THREAD_ERR);
        let ids = remotes.keys().cloned().collect::<Vec<_>>();
        drop(remotes);

        for id in ids {
            self.remove(id);
        }
    }
}

pub trait EventProcessor<C>
where C: Fn(Endpoint, AdapterEvent<'_>)
{
    fn try_process(&mut self, id: ResourceId, event_callback: &mut C);
}

pub struct GenericEventProcessor<R, L> {
    remote_register: Arc<ResourceRegister<R>>,
    listener_register: Arc<ResourceRegister<L>>,
    event_handler: Box<dyn EventHandler<Remote = R, Listener = L>>,
}

impl<R: Source, L: Source> GenericEventProcessor<R, L> {
    pub fn new(
        remote_register: Arc<ResourceRegister<R>>,
        listener_register: Arc<ResourceRegister<L>>,
        event_handler: impl EventHandler<Remote = R, Listener = L> + 'static,
    ) -> GenericEventProcessor<R, L>
    {
        GenericEventProcessor {
            remote_register,
            listener_register,
            event_handler: Box::new(event_handler),
        }
    }
}

impl<C, R: Source, L: Source> EventProcessor<C> for GenericEventProcessor<R, L>
where C: Fn(Endpoint, AdapterEvent<'_>)
{
    fn try_process(&mut self, id: ResourceId, event_callback: &mut C) {
        match id.resource_type() {
            ResourceType::Remote => {
                let remotes = self.remote_register.resources().read().expect(OTHER_THREAD_ERR);
                let to_remove = if let Some((resource, addr)) = remotes.get(&id) {
                    let endpoint = Endpoint::new(id, *addr);
                    loop {
                        let status = self.event_handler.read_event(&resource, &|data| {
                            log::trace!("Read {} bytes from {}", data.len(), id);
                            event_callback(endpoint, AdapterEvent::Data(data));
                        });
                        log::trace!("Processing read event {}, from {}", status, endpoint);
                        match status {
                            ReadStatus::MoreData => continue,
                            ReadStatus::Disconnected => break Some(endpoint),
                            ReadStatus::WaitNextEvent => break None,
                            ReadStatus::Interrupted => continue,
                        };
                    }
                }
                else {
                    None
                };

                drop(remotes);

                if let Some(endpoint) = to_remove {
                    self.remote_register.remove(id);
                    event_callback(endpoint, AdapterEvent::Removed);
                }
            }
            ResourceType::Listener => {
                let listeners = self.listener_register.resources().read().expect(OTHER_THREAD_ERR);

                let remotes = &mut self.remote_register;

                if let Some((resource, _)) = listeners.get(&id) {
                    loop {
                        let status = self.event_handler.acception_event(&resource);
                        log::trace!("Processing accept event {} from {}", status, id);
                        match status {
                            AcceptStatus::AcceptedRemote(addr, remote) => {
                                let id = remotes.add(remote, addr);
                                let endpoint = Endpoint::new(id, addr);
                                event_callback(endpoint, AdapterEvent::Added);
                            }
                            AcceptStatus::AcceptedData(addr, data) => {
                                let endpoint = Endpoint::new(id, addr);
                                event_callback(endpoint, AdapterEvent::Data(data));
                            }
                            AcceptStatus::WaitNextEvent => break,
                            AcceptStatus::Interrupted => continue,
                        }
                    }
                }
            }
        }
    }
}

impl std::fmt::Display for ReadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            ReadStatus::MoreData => "MoreData",
            ReadStatus::Disconnected => "Disconnected",
            ReadStatus::WaitNextEvent => "WaitNextEvent",
            ReadStatus::Interrupted => "Interrupted",
        };
        write!(f, "ReadStatus::{}", string)
    }
}

impl<R> std::fmt::Display for AcceptStatus<'_, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            AcceptStatus::AcceptedRemote(addr, _) => format!("AcceptedRemote({})", addr),
            AcceptStatus::AcceptedData(addr, _) => format!("AcceptedData({})", addr),
            AcceptStatus::WaitNextEvent => "WaitNextEvent".into(),
            AcceptStatus::Interrupted => "Interrupted".into(),
        };
        write!(f, "AcceptStatus::{}", string)
    }
}