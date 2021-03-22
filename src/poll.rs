use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};

use mio::{Poll as MioPoll, Interest, Token, Events, Registry, Waker};
use mio::event::{Source};

use crossbeam::channel::{self, Sender, Receiver};

use std::time::{Duration};
use std::sync::{Arc};
use std::io::{ErrorKind};

pub enum PollEvent<E> {
    Network(ResourceId),
    Waker(E),
}

impl From<Token> for ResourceId {
    fn from(token: Token) -> Self {
        Self::from(token.0 >> Poll::<()>::RESERVED_BITS)
    }
}

impl From<ResourceId> for Token {
    fn from(id: ResourceId) -> Self {
        Token((id.raw() << Poll::<()>::RESERVED_BITS) | 1)
    }
}

pub struct Poll<E> {
    mio_poll: MioPoll,
    events: Events,
    waker: Arc<Waker>,
    event_sender: Sender<E>,
    event_receiver: Receiver<E>,
}

impl<E> Default for Poll<E> {
    fn default() -> Self {
        let mio_poll = MioPoll::new().unwrap();
        let (event_sender, event_receiver) = channel::unbounded();
        Self {
            waker: Arc::new(Waker::new(&mio_poll.registry(), Self::WAKER_TOKEN).unwrap()),
            mio_poll,
            events: Events::with_capacity(Self::EVENTS_SIZE),
            event_sender,
            event_receiver,
        }
    }
}

impl<E> Poll<E> {
    const EVENTS_SIZE: usize = 1024;
    const RESERVED_BITS: usize = 1;
    const WAKER_TOKEN: Token = Token(0);

    pub fn process_event<C>(&mut self, timeout: Option<Duration>, mut event_callback: C)
    where C: FnMut(PollEvent<E>) {
        loop {
            match self.mio_poll.poll(&mut self.events, timeout) {
                Ok(_) => {
                    for mio_event in &self.events {
                        let poll_event = match mio_event.token() {
                            Self::WAKER_TOKEN => {
                                log::trace!("POLL EVENT: waker");
                                // As long as a waker token is received,
                                // there is contain an event in the receiver. So it never blocks.
                                let signal = self.event_receiver.recv().unwrap();
                                PollEvent::Waker(signal)
                            }
                            token => {
                                let resource_id = ResourceId::from(token);
                                log::trace!("POLL EVENT: {}", resource_id);
                                PollEvent::Network(resource_id)
                            }
                        };

                        event_callback(poll_event);
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

    /*
    pub fn create_signaling(&mut self) -> PollEventSender<E>
    {
        PollEventSender::new(self.waker.clone(), self.event_sender.clone())
    }
    */
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

    pub fn add(&self, source: &mut dyn Source) -> ResourceId {
        let id = self.id_generator.generate();
        self.registry.register(source, id.into(), Interest::READABLE).unwrap();
        log::trace!("Register to poll: {}", id);
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

/*
pub struct PollEventSender<E> {
    waker: Arc<Waker>,
    event_sender: Sender<E>,
}
impl<E> PollEventSender<E> {
    pub fn send(&self, event: E) {
        self.sender.send();
        self.waker.wake().unwrap();
        log::trace!("Wake poll...");
    }
}
*/
