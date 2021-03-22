use crate::resource_id::{ResourceId};
use crate::poll::{PollRegistry};
use crate::adapter::{Resource};
use crate::util::{OTHER_THREAD_ERR};

use crossbeam::channel::{self, Sender, Receiver};

use std::collections::{HashMap};
use std::net::{SocketAddr};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

pub struct ResourceRegistry<S> {
    // We store the most significant addr of the resource because if the resource disconnects,
    // it can not be retrieved.
    // If the resource is a remote resource, the addr will be the peer addr.
    // If the resource is a local resource, the addr will be the local addr.
    resources: RwLock<HashMap<ResourceId, Arc<(S, SocketAddr)>>>,
    delayed_removal_sender: Sender<Arc<(S, SocketAddr)>>,
    delayed_removal_receiver: Receiver<Arc<(S, SocketAddr)>>,
    delayed_removal_indicator: Arc<AtomicBool>,
    poll_registry: PollRegistry,
}

impl<S: Resource> ResourceRegistry<S> {
    pub fn new(
        poll_registry: PollRegistry,
        delayed_removal_indicator: Arc<AtomicBool>,
    ) -> ResourceRegistry<S> {
        let (delayed_removal_sender, delayed_removal_receiver) = channel::unbounded();
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            delayed_removal_sender,
            delayed_removal_receiver,
            delayed_removal_indicator,
            poll_registry,
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
            .map(|resource| {
                if self.delayed_removal_indicator.load(Ordering::Relaxed) {
                    // We are potential removing into a nested call of the same resource,
                    // so we can not perform 'Arc::get_mut()i the resource because it is
                    // already using. We can not simply wait because is the same thread which are
                    // using the resource. We are forced to delay the removal.
                    self.delayed_removal_sender.send(resource).unwrap();
                }
                else {
                    // We have not into a nested call using the same resource,
                    // so we need to wait to finish other thread using this resource and remove.
                    Self::wait_to_remove(&self.poll_registry, resource);
                }
            })
            .is_some()
    }

    pub fn get(&self, id: ResourceId) -> Option<Arc<(S, SocketAddr)>> {
        self.resources.read().expect(OTHER_THREAD_ERR).get(&id).cloned()
    }

    pub fn remove_pending_resource(&self) -> bool {
        self.delayed_removal_receiver.try_recv().map(|resource| {
            Self::wait_to_remove(&self.poll_registry, resource);
        }).ok().is_some()
    }

    fn wait_to_remove(poll_registry: &PollRegistry, mut resource: Arc<(S, SocketAddr)>) {
        loop {
            match Arc::get_mut(&mut resource) {
                Some(resource) => break poll_registry.remove(resource.0.source()),
                None => continue,
            }
        }
    }
}
