use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId};
use crate::poll::{Poll};
use crate::adapter::{Adapter, SendStatus};
use crate::remote_addr::{RemoteAddr};
use crate::driver::{AdapterEvent, ActionController, EventProcessor, Driver};
use crate::util::{OTHER_THREAD_ERR};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::io::{self};

type ActionControllers = Vec<Box<dyn ActionController + Send>>;
type EventProcessors = Vec<Box<dyn EventProcessor + Send>>;

/// Used to configured the engine
pub struct AdapterLauncher {
    poll: Poll,
    controllers: ActionControllers,
    processors: EventProcessors,
}

impl Default for AdapterLauncher {
    fn default() -> AdapterLauncher {
        Self {
            poll: Poll::default(),
            controllers: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| {
                    Box::new(UnimplementedActionController) as Box<dyn ActionController + Send>
                })
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| Box::new(UnimplementedEventProcessor) as Box<dyn EventProcessor + Send>)
                .collect(),
        }
    }
}

impl AdapterLauncher {
    /// Mount an adapter associating it with an id.
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter + 'static) {
        let index = adapter_id as usize;

        let driver = Driver::new(adapter, adapter_id, &mut self.poll);

        self.controllers[index] = Box::new(driver.clone()) as Box<(dyn ActionController + Send)>;
        self.processors[index] = Box::new(driver) as Box<(dyn EventProcessor + Send)>;
    }

    /// Consume this instance to obtain the adapter handles.
    fn launch(self) -> (Poll, ActionControllers, EventProcessors) {
        (self.poll, self.controllers, self.processors)
    }
}

/// The core of the system network of message-io.
/// It is in change of managing the adapters, wake up from events and performs the user actions.
pub struct NetworkEngine {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    controllers: ActionControllers,
}

impl NetworkEngine {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

    pub fn new(
        launcher: AdapterLauncher,
        event_callback: impl Fn(Endpoint, AdapterEvent<'_>) + Send + 'static,
    ) -> Self
    {
        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let (poll, controllers, processors) = launcher.launch();

        let thread =
            Self::run_processor(running, poll, processors, move |endpoint, adapter_event| {
                match adapter_event {
                    AdapterEvent::Added => {
                        log::trace!("Endpoint added: {}", endpoint);
                    }
                    AdapterEvent::Data(data) => {
                        log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                    }
                    AdapterEvent::Removed => {
                        log::trace!("Endpoint removed: {}", endpoint);
                    }
                }
                event_callback(endpoint, adapter_event);
            });

        Self { thread: Some(thread), thread_running, controllers }
    }

    pub fn run_processor(
        running: Arc<AtomicBool>,
        mut poll: Poll,
        mut processors: EventProcessors,
        event_callback: impl Fn(Endpoint, AdapterEvent<'_>) + Send + 'static,
    ) -> JoinHandle<()>
    {
        thread::Builder::new()
            .name("message-io: event processor".into())
            .spawn(move || {
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    poll.process_event(timeout, |resource_id| {
                        log::trace!("Resource id {} woke up by an event", resource_id);

                        processors[resource_id.adapter_id() as usize]
                            .try_process(resource_id, &event_callback);
                    });
                }
            })
            .unwrap()
    }

    /// Similar to [`crate::network::Network::connect()`] but it accepts and id.
    pub fn connect(
        &mut self,
        adapter_id: u8,
        addr: RemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)>
    {
        self.controllers[adapter_id as usize].connect(addr).map(|(endpoint, addr)|{
            log::trace!("Connected endpoint {} by {}", endpoint, adapter_id);
            (endpoint, addr)
        })
    }

    /// Similar to [`crate::network::Network::listen()`] but it accepts and id.
    pub fn listen(
        &mut self,
        adapter_id: u8,
        addr: SocketAddr,
    ) -> io::Result<(ResourceId, SocketAddr)>
    {
        self.controllers[adapter_id as usize].listen(addr).map(|(resource_id, addr)| {
            log::trace!("New resource {} listening by {}", resource_id, adapter_id);
            (resource_id, addr)
        })
    }

    /// See [`crate::network::Network::remove_resource()`].
    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        self.controllers[id.adapter_id() as usize].remove(id)
    }

    /// Similar to [`crate::network::Network::send()`] but it accepts a raw message.
    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        let status =
            self.controllers[endpoint.resource_id().adapter_id() as usize].send(endpoint, data);
        log::trace!("Message sent to {}, {:?}", endpoint, status);
        status
    }
}

impl Drop for NetworkEngine {
    fn drop(&mut self) {
        self.thread_running.store(false, Ordering::Relaxed);
        self.thread.take().unwrap().join().expect(OTHER_THREAD_ERR);
    }
}

// The following unimplemented controller/processor is used to fill
// the invalid adapter id holes in the controllers / processors.
// It is faster and cleanest than to use an option that always must to be unwrapped.
// (Even the user can not use bad the API in this context)

const UNIMPLEMENTED_ADAPTER_ERR: &str =
    "The chosen adapter id doesn't reference an existing adapter";

struct UnimplementedActionController;
impl ActionController for UnimplementedActionController {
    fn connect(&mut self, _: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn listen(&mut self, _: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn send(&mut self, _: Endpoint, _: &[u8]) -> SendStatus {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn remove(&mut self, _: ResourceId) -> Option<()> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }
}

struct UnimplementedEventProcessor;
impl EventProcessor for UnimplementedEventProcessor {
    fn try_process(&mut self, _: ResourceId, _: &dyn Fn(Endpoint, AdapterEvent<'_>)) {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }
}
