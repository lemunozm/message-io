use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId};
use crate::poll::{Poll, PollEvent};
use crate::adapter::{Adapter, SendStatus};
use crate::remote_addr::{RemoteAddr};
use crate::driver::{AdapterEvent, ActionController, EventProcessor, Driver};
use crate::util::{OTHER_THREAD_ERR};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::io::{self};

type ActionControllers = Vec<Box<dyn ActionController + Send>>;
type EventProcessors = Vec<Box<dyn EventProcessor + Send>>;

/// Used to configured the engine
pub struct AdapterLauncher {
    poll: Poll<()>,
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
    fn launch(self) -> (Poll<()>, ActionControllers, EventProcessors) {
        (self.poll, self.controllers, self.processors)
    }
}

/// The core of the message-io system network.
/// It is in change of managing the adapters, wake up from events and performs the user actions.
enum NetworkThread {
    Ready(Poll<()>, EventProcessors),
    Running(JoinHandle<(Poll<()>, EventProcessors)>, Arc<AtomicBool>),
}

impl NetworkThread {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

    fn log_adapter_event(adapter_event: &AdapterEvent) {
        match adapter_event {
            AdapterEvent::Added(endpoint, listener_id) => {
                log::trace!("Endpoint added: {} by {}", endpoint, listener_id);
            }
            AdapterEvent::Data(endpoint, data) => {
                log::trace!("Data received from {}, {} bytes", endpoint, data.len());
            }
            AdapterEvent::Removed(endpoint) => {
                log::trace!("Endpoint removed: {}", endpoint);
            }
        }
    }

    fn run_processor(
        running: Arc<AtomicBool>,
        mut poll: Poll<()>,
        processors: EventProcessors,
        event_callback: impl Fn(AdapterEvent<'_>) + Send + 'static,
    ) -> JoinHandle<(Poll<()>, EventProcessors)> {
        thread::Builder::new()
            .name(format!("{}/message-io::engine", thread::current().name().unwrap_or("")))
            .spawn(move || {
                //stop here until run
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    poll.process_event(timeout, |poll_event| match poll_event {
                        PollEvent::Network(resource_id) => {
                            processors[resource_id.adapter_id() as usize]
                                .process(resource_id, &event_callback);
                        }
                        PollEvent::Waker(_event) => {
                            todo!();
                        }
                    });
                }
                (poll, processors)
            })
            .unwrap()
    }

    pub fn run(self, event_callback: impl Fn(AdapterEvent<'_>) + Send + 'static) -> NetworkThread {
        match self {
            NetworkThread::Ready(poll, processors) => {
                let thread_running = Arc::new(AtomicBool::new(true));
                let running = thread_running.clone();
                let thread = Self::run_processor(running, poll, processors, move |adapter_event| {
                    if log::log_enabled!(log::Level::Trace) {
                        Self::log_adapter_event(&adapter_event);
                    }
                    event_callback(adapter_event);
                });
                NetworkThread::Running(thread, thread_running)
            }
            NetworkThread::Running(..) => panic!("Network thread already running"),
        }
    }

    pub fn stop(self) -> NetworkThread {
        match self {
            NetworkThread::Ready(..) => panic!("Network thread is not running"),
            NetworkThread::Running(thread, running) => {
                running.store(false, Ordering::Relaxed);
                let (poll, processors) = thread.join().expect(OTHER_THREAD_ERR);
                NetworkThread::Ready(poll, processors)
            }
        }
    }
}

pub struct NetworkEngine {
    controllers: ActionControllers,
    network_thread: Mutex<Option<NetworkThread>>,
}

impl NetworkEngine {
    pub fn new(launcher: AdapterLauncher) -> Self {
        let (poll, controllers, processors) = launcher.launch();
        let network_thread = Mutex::new(Some(NetworkThread::Ready(poll, processors)));

        Self { network_thread, controllers }
    }

    pub fn run(&self, event_callback: impl Fn(AdapterEvent<'_>) + Send + 'static) {
        let mut network_thread = self.network_thread.lock().unwrap();
        *network_thread = Some(network_thread.take().unwrap().run(event_callback));
        log::trace!("Network thread running");
    }

    pub fn stop(&self) {
        let mut network_thread = self.network_thread.lock().unwrap();
        *network_thread = Some(network_thread.take().unwrap().stop());
        log::trace!("Network thread stopped");
    }

    pub fn is_running(&self) -> bool {
        let network_thread = self.network_thread.lock().unwrap();
        match network_thread.as_ref().unwrap() {
            NetworkThread::Ready(..) => false,
            NetworkThread::Running(..) => true,
        }
    }

    pub fn connect(&self, adapter_id: u8, addr: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        log::trace!("Connect to {} by adapter: {}", addr, adapter_id);
        self.controllers[adapter_id as usize].connect(addr).map(|(endpoint, addr)| {
            log::trace!("Connected endpoint {}", endpoint);
            (endpoint, addr)
        })
    }

    pub fn listen(&self, adapter_id: u8, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        log::trace!("Listen to {} by adapter: {}", addr, adapter_id);
        self.controllers[adapter_id as usize].listen(addr).map(|(resource_id, addr)| {
            log::trace!("Listening by {}", resource_id);
            (resource_id, addr)
        })
    }

    pub fn remove(&self, id: ResourceId) -> bool {
        let value = self.controllers[id.adapter_id() as usize].remove(id);
        log::trace!("Remove {}: {}", id, value);
        value
    }

    pub fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        let status =
            self.controllers[endpoint.resource_id().adapter_id() as usize].send(endpoint, data);
        log::trace!("Send {} bytes to {}: {:?}", data.len(), endpoint, status);
        status
    }
}

impl Drop for NetworkEngine {
    fn drop(&mut self) {
        if self.is_running() {
            self.stop();
        }
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
