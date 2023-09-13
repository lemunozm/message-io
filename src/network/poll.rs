use super::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};

use mio::{Poll as MioPoll, Interest, Token, Events, Registry, Waker};
use mio::event::{Source};

use std::time::{Duration};
use std::sync::{Arc};
use std::io::{ErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Used for the adapter implementation.
/// Specify the kind of event that is available for a resource.
pub enum Readiness {
    /// The resource is available to write
    Write,

    /// The resource is available to read (has any content to read).
    Read,
}

pub enum PollEvent {
    Network(ResourceId, Readiness),
    Waker,
}

impl From<Token> for ResourceId {
    fn from(token: Token) -> Self {
        (token.0 >> Poll::RESERVED_BITS).into()
    }
}

impl From<ResourceId> for Token {
    fn from(id: ResourceId) -> Self {
        Token((id.raw() << Poll::RESERVED_BITS) | 1)
    }
}

pub struct Poll {
    mio_poll: MioPoll,
    events: Events,
    #[allow(dead_code)] //TODO: remove it with poll native event support
    waker: Arc<Waker>,
}

impl Default for Poll {
    fn default() -> Self {
        let mio_poll = MioPoll::new().unwrap();
        Self {
            waker: Arc::new(Waker::new(mio_poll.registry(), Self::WAKER_TOKEN).unwrap()),
            mio_poll,
            events: Events::with_capacity(Self::EVENTS_SIZE),
        }
    }
}

impl Poll {
    const EVENTS_SIZE: usize = 1024;
    const RESERVED_BITS: usize = 1;
    const WAKER_TOKEN: Token = Token(0);

    pub fn process_event<C>(&mut self, timeout: Option<Duration>, mut event_callback: C)
    where C: FnMut(PollEvent) {
        loop {
            match self.mio_poll.poll(&mut self.events, timeout) {
                Ok(()) => {
                    for mio_event in &self.events {
                        if Self::WAKER_TOKEN == mio_event.token() {
                            log::trace!("POLL WAKER EVENT");
                            event_callback(PollEvent::Waker);
                        }
                        else {
                            let id = ResourceId::from(mio_event.token());
                            if mio_event.is_readable() {
                                log::trace!("POLL EVENT (R): {}", id);
                                event_callback(PollEvent::Network(id, Readiness::Read));
                            }
                            if mio_event.is_writable() {
                                log::trace!("POLL EVENT (W): {}", id);
                                event_callback(PollEvent::Network(id, Readiness::Write));
                            }
                        }
                    }
                    break
                }
                Err(ref err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(ref err) => panic!("{}: No error here", err),
            }
        }
    }

    pub fn create_registry(&mut self, adapter_id: u8, resource_type: ResourceType) -> PollRegistry {
        PollRegistry::new(adapter_id, resource_type, self.mio_poll.registry().try_clone().unwrap())
    }

    #[allow(dead_code)] //TODO: remove it with poll native event support
    pub fn create_waker(&mut self) -> PollWaker {
        PollWaker::new(self.waker.clone())
    }
}

pub struct PollRegistry {
    id_generator: Arc<ResourceIdGenerator>,
    registry: Registry,
}

impl PollRegistry {
    fn new(adapter_id: u8, resource_type: ResourceType, registry: Registry) -> Self {
        Self {
            id_generator: Arc::new(ResourceIdGenerator::new(adapter_id, resource_type)),
            registry,
        }
    }

    pub fn add(&self, source: &mut dyn Source, write_readiness: bool) -> ResourceId {
        let id = self.id_generator.generate();
        let interest = match write_readiness {
            true => Interest::READABLE | Interest::WRITABLE,
            false => Interest::READABLE,
        };
        self.registry.register(source, id.into(), interest).unwrap();
        id
    }

    pub fn remove(&self, source: &mut dyn Source) {
        self.registry.deregister(source).unwrap()
    }
}

impl Clone for PollRegistry {
    fn clone(&self) -> Self {
        Self {
            id_generator: self.id_generator.clone(),
            registry: self.registry.try_clone().unwrap(),
        }
    }
}

#[allow(dead_code)] //TODO: remove it with poll native event support
pub struct PollWaker {
    waker: Arc<Waker>,
}

impl PollWaker {
    #[allow(dead_code)] //TODO: remove it with poll native event support
    fn new(waker: Arc<Waker>) -> Self {
        Self { waker }
    }

    #[allow(dead_code)] //TODO: remove it with poll native event support
    pub fn wake(&self) {
        self.waker.wake().unwrap();
        log::trace!("Wake poll...");
    }
}

impl Clone for PollWaker {
    fn clone(&self) -> Self {
        Self { waker: self.waker.clone() }
    }
}
