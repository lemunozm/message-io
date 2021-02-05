use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId, ResourceType};
use crate::poll::{Poll};
use crate::adapter::{Adapter, Controller, Processor, AdapterEvent};
use crate::util::{OTHER_THREAD_ERR, SendingStatus};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::io::{self};

type Controllers = Vec<Box<dyn Controller + Send>>;
type Processors<C> = Vec<Box<dyn Processor<C> + Send>>;

pub struct AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    poll: Poll,
    controllers: Controllers,
    processors: Processors<C>,
}

impl<C> Default for AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    fn default() -> AdapterLauncher<C> {
        Self {
            poll: Poll::default(),
            controllers: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| Box::new(UnimplementedController) as Box<dyn Controller + Send>)
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::ADAPTER_ID_MAX)
                .map(|_| Box::new(UnimplementedProcessor) as Box<dyn Processor<C> + Send>)
                .collect(),
        }
    }
}

impl<C> AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter<C> + 'static) {
        let index = adapter_id as usize;
        let (controller, processor) = adapter.split(self.poll.create_register(adapter_id));
        self.controllers[index] = Box::new(controller);
        self.processors[index] = Box::new(processor);
    }

    fn launch(self) -> (Poll, Controllers, Processors<C>) {
        (self.poll, self.controllers, self.processors)
    }
}

pub struct NetworkEngine {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    controllers: Controllers,
}

impl NetworkEngine {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

    pub fn new<C>(launcher: AdapterLauncher<C>, mut event_callback: C) -> Self
    where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static {
        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let (mut poll, controllers, mut processors) = launcher.launch();

        let thread = thread::Builder::new()
            .name("message-io: event processor".into())
            .spawn(move || {
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    poll.process_event(timeout, &mut |resource_id: ResourceId| {
                        log::trace!("process event for {}. ", resource_id);
                        let processor = &mut processors[resource_id.adapter_id() as usize];

                        match resource_id.resource_type() {
                            ResourceType::Listener => {
                                processor.process_listener(resource_id, &mut event_callback)
                            }
                            ResourceType::Remote => {
                                processor.process_remote(resource_id, &mut event_callback)
                            }
                        }
                    });
                }
            })
            .unwrap();

        Self { thread: Some(thread), thread_running, controllers }
    }

    pub fn connect(&mut self, adapter_id: u8, addr: SocketAddr) -> io::Result<Endpoint> {
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

    pub fn local_addr(&self, id: ResourceId) -> Option<SocketAddr> {
        self.controllers[id.adapter_id() as usize].local_addr(id)
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus {
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
// the invalid adapter id holes in the registers.
// It is faster and cleanest than to use an option that always must to be unwrapped.
const UNIMPLEMENTED_ADAPTER_ERR: &str = "The adapter id do not reference an existing adapter";

struct UnimplementedController;
impl Controller for UnimplementedController {
    fn connect(&mut self, _: SocketAddr) -> io::Result<Endpoint> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn listen(&mut self, _: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn remove(&mut self, _: ResourceId) -> Option<()> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn local_addr(&self, _: ResourceId) -> Option<SocketAddr> {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn send(&mut self, _: Endpoint, _: &[u8]) -> SendingStatus {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }
}

struct UnimplementedProcessor;
impl<C> Processor<C> for UnimplementedProcessor
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    fn process_listener(&mut self, _: ResourceId, _: &mut C) {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }

    fn process_remote(&mut self, _: ResourceId, _: &mut C) {
        panic!(UNIMPLEMENTED_ADAPTER_ERR);
    }
}
