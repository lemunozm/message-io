use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::{OTHER_THREAD_ERR};

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
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(poll_registry: PollRegistry) -> ResourceRegistry<S> {
        ResourceRegistry { resources: RwLock::new(HashMap::new()), poll_registry }
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
            .map(|mut resource| {
                loop {
                    // Yeah, it is an active waiting, but relax:
                    // You will only fail to get the Arc access when the resource is either
                    // reading or writing.
                    // This active waiting only wait to finish those read/write operations
                    // to remove it.
                    // Since this is under the write() of the resource, you only will wait
                    // to the last pending operation, if there was, that is exactly you want.
                    match Arc::get_mut(&mut resource) {
                        Some(resource) => break self.poll_registry.remove(resource.0.source()),
                        None => continue,
                    }
                }
            })
            .is_some()
    }

    pub fn get(&self, id: ResourceId) -> Option<Arc<(S, SocketAddr)>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).cloned()
    }
}
