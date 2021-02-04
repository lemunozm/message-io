use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};
use crate::util::{OTHER_THREAD_ERR};

use mio::{Poll, Interest, Token, Events, Registry};
use mio::event::{Source};

use std::time::{Duration};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::io::{ErrorKind};

pub enum AdapterEvent<'a> {
    Added,
    Data(&'a [u8]),
    Removed,
}

pub struct MioPoll {
    poll: Poll,
    events: Events,
}

impl MioPoll {
    const EVENTS_SIZE: usize = 1024;

    fn process_event<C>(&mut self, timeout: Option<Duration>, event_callback: &mut C)
    where C: FnMut(ResourceId) {
        loop {
            match self.poll.poll(&mut self.events, timeout) {
                Ok(_) => {
                    for mio_event in &self.events {
                        let id = ResourceId::from(mio_event.token().0);
                        log::trace!("Wake from poll for resource id {}. ", id);
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

    pub fn create_register(&mut self, adapter_id: u8) -> MioRegister {
        MioRegister::new(adapter_id, self.poll.registry().try_clone().unwrap())
    }
}

impl Default for MioPoll {
    fn default() -> Self {
        Self { poll: Poll::new().unwrap(), events: Events::with_capacity(Self::EVENTS_SIZE) }
    }
}

pub struct MioRegister {
    id_generator: Arc<ResourceIdGenerator>,
    registry: Registry,
}

impl MioRegister {
    fn new(adapter_id: u8, registry: Registry) -> MioRegister {
        MioRegister { id_generator: Arc::new(ResourceIdGenerator::new(adapter_id)), registry }
    }

    pub fn add(&mut self, source: &mut dyn Source, resource_type: ResourceType) -> ResourceId {
        let id = self.id_generator.generate(resource_type);
        self.registry.register(source, Token(id.raw()), Interest::READABLE).unwrap();
        id
    }

    pub fn remove(&mut self, source: &mut dyn Source) {
        self.registry.deregister(source).unwrap()
    }

    pub fn adapter_id(&self) -> u8 {
        self.id_generator.adapter_id()
    }
}

impl Clone for MioRegister {
    fn clone(&self) -> Self {
        MioRegister {
            id_generator: self.id_generator.clone(),
            registry: self.registry.try_clone().unwrap(),
        }
    }
}

pub struct MioEngine {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
}

impl MioEngine {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
    pub fn new<C>(mut mio_poll: MioPoll, mut event_callback: C) -> MioEngine
    where C: FnMut(ResourceId) + Send + 'static {
        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let thread = thread::Builder::new()
            .name("message-io: tcp-adapter".into())
            .spawn(move || {
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    mio_poll.process_event(timeout, &mut event_callback);
                }
            })
            .unwrap();

        Self { thread: Some(thread), thread_running }
    }
}

impl Drop for MioEngine {
    fn drop(&mut self) {
        self.thread_running.store(false, Ordering::Relaxed);
        self.thread.take().unwrap().join().expect(OTHER_THREAD_ERR);
    }
}
