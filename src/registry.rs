use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::{OTHER_THREAD_ERR};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

pub struct Register<S> {
    pub resource: S,
    pub addr: SocketAddr,
    delayed_removal: Arc<AtomicBool>,
    must_remove: Arc<AtomicBool>,
}

impl<S> Register<S> {
    fn new(resource: S, addr: SocketAddr) -> Self {
        Self {
            resource,
            addr,
            delayed_removal: Arc::new(AtomicBool::new(false)),
            must_remove: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct ResourceRegistry<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, Arc<Register<S>>>>,
    poll_registry: PollRegistry,
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(poll_registry: PollRegistry) -> ResourceRegistry<S> {
        ResourceRegistry { resources: RwLock::new(HashMap::new()), poll_registry }
    }

    /// Add a resource into the registry.
    pub fn add(&self, mut resource: S, addr: SocketAddr) -> ResourceId {
        let id = self.poll_registry.add(resource.source());
        let register = Register::new(resource, addr);
        self.resources.write().expect(OTHER_THREAD_ERR).insert(id, Arc::new(register));
        id
    }

    /// Remove a register from the registry.
    /// This function ensure that the register is removed from the registry,
    /// but not the destruction of the resource itself.
    /// Because the resource is shared the removing will be delayed until the last reference.
    pub fn remove(&self, id: ResourceId) -> bool {
        self.resources
            .write()
            .expect(OTHER_THREAD_ERR)
            .remove(&id)
            .map(|mut register| {
                if register.delayed_removal.load(Ordering::Relaxed) {
                    log::trace!("Remove mode: delayed");
                    // We are potential removing into a nested call of the same registry,
                    // so we can not perform 'Arc::get_mut()' over the registry because it is
                    // already using. We can not simply wait because is the same thread which are
                    // using the resource. We are forced to delay the removal.
                    register.must_remove.store(false, Ordering::Relaxed);
                }
                else {
                    log::trace!("Remove mode: blocked");
                    // We have not into a nested call using the same resource,
                    // so we need to wait to finish other thread using this resource and remove.
                    Self::wait_to_remove(&self.poll_registry, &mut register);
                }
            })
            .is_some()
    }

    /// Returned a shared reference of the register.
    pub fn get(&self, id: ResourceId) -> Option<Arc<Register<S>>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).cloned()
    }

    /// As get, but the register is returned protected against removings.
    /// If a removed is performed during the lifetime of `ProtectedResource`,
    /// the removing is delayed until its drop.
    pub fn get_protected(&self, id: ResourceId) -> Option<ProtectedResource<S>> {
        self.resources
            .read()
            .expect(OTHER_THREAD_ERR)
            .get(&id)
            .map(|register| ProtectedResource::new(register.clone(), &self.poll_registry))
    }

    fn wait_to_remove(poll_registry: &PollRegistry, register: &mut Arc<Register<S>>) {
        loop {
            match Arc::get_mut(register) {
                Some(register) => break poll_registry.remove(register.resource.source()),
                None => continue,
            }
        }
    }
}

/// Ensure that no removings are performed during the lifetime of the guard.
pub struct ProtectedResource<'a, S: Resource> {
    register: Arc<Register<S>>,
    poll_registry: &'a PollRegistry,
}

impl<'a, S: Resource> ProtectedResource<'a, S> {
    fn new(register: Arc<Register<S>>, poll_registry: &'a PollRegistry) -> Self {
        register.delayed_removal.store(true, Ordering::Relaxed);
        Self { register, poll_registry }
    }
}

impl<'a, S: Resource> Drop for ProtectedResource<'a, S> {
    fn drop(&mut self) {
        self.register.delayed_removal.store(false, Ordering::Relaxed);
        if self.register.must_remove.load(Ordering::Relaxed) {
            ResourceRegistry::wait_to_remove(self.poll_registry, &mut self.register);
        }
    }
}

impl<'a, S: Resource> std::ops::Deref for ProtectedResource<'a, S> {
    type Target = Arc<Register<S>>;

    fn deref(&self) -> &Arc<Register<S>> {
        &self.register
    }
}
