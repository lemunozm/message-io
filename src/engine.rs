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
type EventProcessors<C> = Vec<Box<dyn EventProcessor<C> + Send>>;

pub struct AdapterLauncher<C> {
    poll: Poll,
    controllers: ActionControllers,
    processors: EventProcessors<C>,
}

impl<C> Default for AdapterLauncher<C>
where C: Fn(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    fn default() -> AdapterLauncher<C> {
        Self {
            poll: Poll::default(),
            controllers: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| {
                    Box::new(UnimplementedActionController) as Box<dyn ActionController + Send>
                })
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| Box::new(UnimplementedEventProcessor) as Box<dyn EventProcessor<C> + Send>)
                .collect(),
        }
    }
}

impl<C> AdapterLauncher<C>
where C: Fn(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter + 'static) {
        let index = adapter_id as usize;

        let driver = Driver::new(adapter, adapter_id, &mut self.poll);

        self.controllers[index] = Box::new(driver.clone()) as Box<(dyn ActionController + Send)>;
        self.processors[index] = Box::new(driver) as Box<(dyn EventProcessor<C> + Send)>;
    }

    fn launch(self) -> (Poll, ActionControllers, EventProcessors<C>) {
        (self.poll, self.controllers, self.processors)
    }
}

pub struct NetworkEngine {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    controllers: ActionControllers,
}

impl NetworkEngine {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

    pub fn new<C>(launcher: AdapterLauncher<C>, mut event_callback: C) -> Self
    where C: Fn(Endpoint, AdapterEvent<'_>) + Send + 'static {
        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let (mut poll, controllers, mut processors) = launcher.launch();

        let thread = thread::Builder::new()
            .name("message-io: event processor".into())
            .spawn(move || {
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    poll.process_event(timeout, &mut |resource_id| {
                        log::trace!("Try process event for {}", resource_id);

                        processors[resource_id.adapter_id() as usize]
                            .try_process(resource_id, &mut event_callback);
                    });
                }
            })
            .unwrap();

        Self { thread: Some(thread), thread_running, controllers }
    }

    pub fn connect(
        &mut self,
        adapter_id: u8,
        addr: RemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)>
    {
        self.controllers[adapter_id as usize].connect(addr)
    }

    pub fn listen(
        &mut self,
        adapter_id: u8,
        addr: SocketAddr,
    ) -> io::Result<(ResourceId, SocketAddr)>
    {
        self.controllers[adapter_id as usize].listen(addr)
    }

    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        self.controllers[id.adapter_id() as usize].remove(id)
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        self.controllers[endpoint.resource_id().adapter_id() as usize].send(endpoint, data)
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

pub struct UnimplementedActionController;
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

pub struct UnimplementedEventProcessor;
impl<C> EventProcessor<C> for UnimplementedEventProcessor
where C: Fn(Endpoint, AdapterEvent<'_>)
{
    fn try_process(&mut self, _: ResourceId, _: &mut C) {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }
}
