use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};

use mio::{Poll as MioPoll, Interest, Token, Events, Registry};
use mio::event::{Source};

use std::time::{Duration};
use std::sync::{Arc};
use std::io::{ErrorKind};

pub struct Poll {
    mio_poll: MioPoll,
    events: Events,
}

impl Poll {
    const EVENTS_SIZE: usize = 1024;

    pub fn process_event<C>(&mut self, timeout: Option<Duration>, mut event_callback: C)
    where C: FnMut(ResourceId) {
        loop {
            match self.mio_poll.poll(&mut self.events, timeout) {
                Ok(_) => {
                    for mio_event in &self.events {
                        let id = ResourceId::from(mio_event.token().0);
                        event_callback(id);
                    }
                    break
                }
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(ref err) => Err(err).expect("No error here"),
            }
        }
    }

    pub fn create_registry(&mut self, adapter_id: u8, resource_type: ResourceType) -> PollRegistry {
        PollRegistry::new(adapter_id, resource_type, self.mio_poll.registry().try_clone().unwrap())
    }
}

impl Default for Poll {
    fn default() -> Self {
        Self { mio_poll: MioPoll::new().unwrap(), events: Events::with_capacity(Self::EVENTS_SIZE) }
    }
}

pub struct PollRegistry {
    id_generator: Arc<ResourceIdGenerator>,
    registry: Registry,
}

impl PollRegistry {
    fn new(adapter_id: u8, resource_type: ResourceType, registry: Registry) -> PollRegistry {
        PollRegistry {
            id_generator: Arc::new(ResourceIdGenerator::new(adapter_id, resource_type)),
            registry,
        }
    }

    pub fn add(&self, source: &mut dyn Source) -> ResourceId {
        let id = self.id_generator.generate();
        self.registry.register(source, Token(id.raw()), Interest::READABLE).unwrap();
        id
    }

    pub fn remove(&self, source: &mut dyn Source) {
        self.registry.deregister(source).unwrap()
    }
}

impl Clone for PollRegistry {
    fn clone(&self) -> Self {
        PollRegistry {
            id_generator: self.id_generator.clone(),
            registry: self.registry.try_clone().unwrap(),
        }
    }
}
