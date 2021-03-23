use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::thread::{OTHER_THREAD_ERR};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{Arc, RwLock};

pub struct Register<S: Resource> {
    pub resource: S,
    pub addr: SocketAddr,
    poll_registry: Arc<PollRegistry>,
}

impl<S: Resource> Register<S> {
    fn new(resource: S, addr: SocketAddr, poll_registry: Arc<PollRegistry>) -> Self {
        Self { resource, addr, poll_registry }
    }
}

impl<S: Resource> Drop for Register<S> {
    fn drop(&mut self) {
        self.poll_registry.remove(self.resource.source());
    }
}

pub struct ResourceRegistry<S: Resource> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, Arc<Register<S>>>>,
    poll_registry: Arc<PollRegistry>,
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(poll_registry: PollRegistry) -> Self {
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            poll_registry: Arc::new(poll_registry),
        }
    }

    /// Add a resource into the registry.
    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_registry.add(resource.source());
        let register = Register::new(resource, addr, self.poll_registry.clone());
        self.resources.write().expect(OTHER_THREAD_ERR).insert(id, Arc::new(register));
        id
    }

    /// Remove a register from the registry.
    /// This function ensure that the register is removed from the registry,
    /// but not the destruction of the resource itself.
    /// Because the resource is shared, the destruction will be delayed until the last reference.
    pub fn remove(&self, id: ResourceId) -> bool {
        self.resources.write().expect(OTHER_THREAD_ERR).remove(&id).is_some()
    }

    /// Returned a shared reference of the register.
    pub fn get(&self, id: ResourceId) -> Option<Arc<Register<S>>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).cloned()
    }
}
