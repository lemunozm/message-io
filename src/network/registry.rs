use super::resource_id::{ResourceId};
use super::poll::{PollRegistry};
use super::adapter::{Resource};

use crate::util::thread::{OTHER_THREAD_ERR};

use std::collections::{HashMap};
use std::sync::{Arc, RwLock};

pub struct Register<S: Resource, P> {
    pub resource: S,
    pub properties: P,

    poll_registry: Arc<PollRegistry>,
}

impl<S: Resource, P> Register<S, P> {
    fn new(resource: S, properties: P, poll_registry: Arc<PollRegistry>) -> Self {
        Self { resource, properties, poll_registry }
    }
}

impl<S: Resource, P> Drop for Register<S, P> {
    fn drop(&mut self) {
        self.poll_registry.remove(self.resource.source());
    }
}

pub struct ResourceRegistry<S: Resource, P> {
    resources: RwLock<HashMap<ResourceId, Arc<Register<S, P>>>>,
    poll_registry: Arc<PollRegistry>,
}

impl<S: Resource, P> ResourceRegistry<S, P> {
    pub fn new(poll_registry: PollRegistry) -> Self {
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            poll_registry: Arc::new(poll_registry),
        }
    }

    /// Add a resource into the registry.
    pub fn register(&self, mut resource: S, properties: P, write_readiness: bool) -> ResourceId {
        // The registry must be locked for the entire implementation to avoid the poll
        // to generate events over not yet registered resources.
        let mut registry = self.resources.write().expect(OTHER_THREAD_ERR);
        let id = self.poll_registry.add(resource.source(), write_readiness);
        let register = Register::new(resource, properties, self.poll_registry.clone());
        registry.insert(id, Arc::new(register));
        id
    }

    /// Remove a register from the registry.
    /// This function ensure that the register is removed from the registry,
    /// but not the destruction of the resource itself.
    /// Because the resource is shared, the destruction will be delayed until the last reference.
    pub fn deregister(&self, id: ResourceId) -> bool {
        self.resources.write().expect(OTHER_THREAD_ERR).remove(&id).is_some()
    }

    /// Returned a shared reference of the register.
    pub fn get(&self, id: ResourceId) -> Option<Arc<Register<S, P>>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).cloned()
    }
}
