use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::{OTHER_THREAD_ERR};

use crossbeam::channel::{self, Sender, Receiver};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{RwLock, TryLockError};

pub struct ResourceRegistry<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, (S, SocketAddr)>>,
    poll_registry: PollRegistry,
    future_insertions: Sender<(ResourceId, S, SocketAddr)>,
    pending_insertions: Receiver<(ResourceId, S, SocketAddr)>,
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(poll_registry: PollRegistry) -> ResourceRegistry<S> {
        let (sender, receiver) = channel::unbounded();
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            poll_registry,
            future_insertions: sender,
            pending_insertions: receiver,
        }
    }

    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_registry.add(resource.source());
        match self.resources.try_write() {
            Ok(ref mut resources) => {
                resources.insert(id, (resource, addr));
            }
            Err(TryLockError::WouldBlock) => {
                self.future_insertions.send((id, resource, addr)).unwrap();
            }
            Err(TryLockError::Poisoned(_)) => panic!("{}", OTHER_THREAD_ERR),
        }
        id
    }

    pub fn remove(&self, id: ResourceId) -> bool {
        let poll_registry = &self.poll_registry;
        self.resources
            .write()
            .expect(OTHER_THREAD_ERR)
            .remove(&id)
            .map(|(mut resource, _)| poll_registry.remove(resource.source()))
            .is_some()
    }

    pub fn resources(&self) -> &RwLock<HashMap<ResourceId, (S, SocketAddr)>> {
        &self.resources
    }

    pub fn register_pending(&self) {
        if !self.pending_insertions.is_empty() {
            let mut resources = self.resources.write().expect(OTHER_THREAD_ERR);
            for (id, resource, addr) in &self.pending_insertions {
                resources.insert(id, (resource, addr));
            }
        }
    }
}

