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

    pub fn process_event<C>(&mut self, timeout: Option<Duration>, event_callback: &mut C)
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
                Err(e) => match e.kind() {
                    ErrorKind::Interrupted => continue,
                    _ => Err(e).expect("No error here"),
                },
            }
        }
    }

    pub fn create_register(&mut self, adapter_id: u8, resource_type: ResourceType) -> PollRegister {
        PollRegister::new(adapter_id, resource_type, self.mio_poll.registry().try_clone().unwrap())
    }
}

impl Default for Poll {
    fn default() -> Self {
        Self { mio_poll: MioPoll::new().unwrap(), events: Events::with_capacity(Self::EVENTS_SIZE) }
    }
}

pub struct PollRegister {
    id_generator: Arc<ResourceIdGenerator>,
    registry: Registry,
}

impl PollRegister {
    fn new(adapter_id: u8, resource_type: ResourceType, registry: Registry) -> PollRegister {
        PollRegister {
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

impl Clone for PollRegister {
    fn clone(&self) -> Self {
        PollRegister {
            id_generator: self.id_generator.clone(),
            registry: self.registry.try_clone().unwrap(),
        }
    }
}
