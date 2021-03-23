use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId};
use crate::poll::{Poll, PollEvent};
use crate::adapter::{Adapter, SendStatus};
use crate::remote_addr::{RemoteAddr};
use crate::driver::{AdapterEvent, ActionController, EventProcessor, Driver};
use crate::network_thread::{NetworkThread};

use std::net::{SocketAddr};
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
            controllers: (0..ResourceId::MAX_ADAPTERS)
                .map(|_| {
                    Box::new(UnimplementedActionController) as Box<dyn ActionController + Send>
                })
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::MAX_ADAPTERS)
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

pub struct NetworkEngine {
    controllers: ActionControllers,
    network_thread: NetworkThread<EventProcessors>,
}

impl NetworkEngine {
    pub fn new(launcher: AdapterLauncher) -> Self {
        let (poll, controllers, processors) = launcher.launch();
        let network_thread = NetworkThread::new(poll, processors);

        Self { network_thread, controllers }
    }

    pub fn run(&self, event_callback: impl Fn(AdapterEvent<'_>) + Send + 'static) {
        self.network_thread.run(move |(poll_event, processors)| {
            match poll_event {
                PollEvent::Network(resource_id) => {
                    let adapter_id = resource_id.adapter_id() as usize;
                    processors[adapter_id].process(resource_id, &|adapter_event| {
                        match adapter_event {
                            AdapterEvent::Added(endpoint, listener_id) => {
                                log::trace!("Endpoint added: {} by {}", endpoint, listener_id);
                            }
                            AdapterEvent::Data(endpoint, data) => {
                                log::trace!("Data from {}, {} bytes", endpoint, data.len());
                            }
                            AdapterEvent::Removed(endpoint) => {
                                log::trace!("Endpoint removed: {}", endpoint);
                            }
                        }
                        event_callback(adapter_event);
                    });
                }
                #[allow(dead_code)] //TODO: remove it with native event support
                PollEvent::Waker => todo!(),
            };
        });
    }

    #[allow(dead_code)] //TODO: remove it with node API feature
    pub fn is_running(&self) -> bool {
        self.network_thread.is_running()
    }

    #[allow(dead_code)] //TODO: remove it with node API feature
    pub fn stop(&self) {
        self.network_thread.stop()
    }

    #[allow(dead_code)] //TODO: remove it with node API feature
    pub fn join(&self) {
        self.network_thread.join()
    }

    pub fn connect(&self, adapter_id: u8, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        log::trace!("Connect to {} by adapter: {}", addr, adapter_id);
        self.controllers[adapter_id as usize].connect(addr).map(|(endpoint, addr)| {
            log::trace!("Connected to {}", endpoint);
            (endpoint, addr)
        })
    }

    pub fn listen(&self, adapter_id: u8, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        log::trace!("Listen by {} by adapter: {}", addr, adapter_id);
        self.controllers[adapter_id as usize].listen(addr).map(|(resource_id, addr)| {
            log::trace!("Listening by {}", resource_id);
            (resource_id, addr)
        })
    }

    pub fn remove(&self, id: ResourceId) -> bool {
        log::trace!("Remove {}", id);
        let value = self.controllers[id.adapter_id() as usize].remove(id);
        log::trace!("Removed: {}", value);
        value
    }

    pub fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        log::trace!("Send {} bytes to {}", data.len(), endpoint);
        let status =
            self.controllers[endpoint.resource_id().adapter_id() as usize].send(endpoint, data);
        log::trace!("Send status: {:?}", status);
        status
    }
}

// The following unimplemented controller/processor is used to fill
// the invalid adapter id holes in the controllers / processors.
// It is faster and cleanest than to use an option that always must to be unwrapped.

const UNIMPLEMENTED_ADAPTER_ERR: &str =
    "The chosen adapter id doesn't reference an existing adapter";

struct UnimplementedActionController;
impl ActionController for UnimplementedActionController {
    fn connect(&self, _: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn listen(&self, _: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn send(&self, _: Endpoint, _: &[u8]) -> SendStatus {
        panic!("{}", UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn remove(&self, _: ResourceId) -> bool {
        panic!("{}", UNIMPLEMENTED_ADAPTER_ERR);
    }
}

struct UnimplementedEventProcessor;
impl EventProcessor for UnimplementedEventProcessor {
    fn process(&self, _: ResourceId, _: &dyn Fn(AdapterEvent<'_>)) {
        panic!("{}", UNIMPLEMENTED_ADAPTER_ERR);
    }
}
