use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::{OTHER_THREAD_ERR};

use crossbeam::channel::{self, Sender, Receiver};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{Arc, RwLock};

pub struct ResourceRegistry<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, Arc<(S, SocketAddr)>>>,
    poll_registry: PollRegistry,
    future_removings: Sender<Arc<(S, SocketAddr)>>,
    pending_removings: Receiver<Arc<(S, SocketAddr)>>,
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(poll_registry: PollRegistry) -> ResourceRegistry<S> {
        let (sender, receiver) = channel::unbounded();
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            poll_registry,
            future_removings: sender,
            pending_removings: receiver,
        }
    }

    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_registry.add(resource.source());
        self.resources.write().expect(OTHER_THREAD_ERR).insert(id, Arc::new((resource, addr)));
        id
    }

    pub fn remove(&self, id: ResourceId) -> bool {
        self.resources
            .write()
            .expect(OTHER_THREAD_ERR)
            .remove(&id)
            .map(|resource| self.future_removings.send(resource).unwrap())
            .is_some()
    }

    pub fn get(&self, id: ResourceId) -> Option<Arc<(S, SocketAddr)>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).map(|resource| resource.clone())
    }

    pub fn finish_pending_removings(&self) {
        for mut resource in self.pending_removings.try_iter() {
            match Arc::get_mut(&mut resource) {
                Some(resource) => self.poll_registry.remove(resource.0.source()),
                // Really rare case when there was a disconnect while
                // the user was trying to write data.
                // We can not remove yet, so we enque again.
                None => self.future_removings.send(resource).unwrap(),
            }
        }
    }
}
