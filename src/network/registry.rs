use super::resource_id::{ResourceId};
use super::poll::{PollRegistry};
use super::adapter::{Resource};

use crate::util::thread::{OTHER_THREAD_ERR};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};

pub struct Register<S: Resource> {
    pub resource: S,

    // Most significant addr of the resource
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    pub addr: SocketAddr,
    ready: AtomicBool,
    poll_registry: Arc<PollRegistry>,
}

impl<S: Resource> Register<S> {
    fn new(resource: S, addr: SocketAddr, ready: bool, poll_registry: Arc<PollRegistry>) -> Self {
        Self { resource, addr, ready: AtomicBool::new(ready), poll_registry }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn mark_as_ready(&self) {
        self.ready.store(true, Ordering::Relaxed);
    }
}

impl<S: Resource> Drop for Register<S> {
    fn drop(&mut self) {
        self.poll_registry.remove(self.resource.source());
    }
}

pub struct ResourceRegistry<S: Resource> {
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
    pub fn add(&self, mut resource: S, addr: SocketAddr, ready: bool) -> ResourceId {
        // The registry must be locked for the entire implementation to avoid
        // poll to generate events over false unregistered resources.
        let mut registry = self.resources.write().expect(OTHER_THREAD_ERR);
        let id = self.poll_registry.add(resource.source());
        let register = Register::new(resource, addr, ready, self.poll_registry.clone());
        registry.insert(id, Arc::new(register));
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
