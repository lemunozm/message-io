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

pub struct AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    poll: Poll,
    controllers: [Option<Box<dyn Controller + Send>>; ResourceId::ADAPTER_ID_MAX],
    processors: [Option<Box<dyn Processor<C> + Send>>; ResourceId::ADAPTER_ID_MAX],
}

impl<C> Default for AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    fn default() -> AdapterLauncher<C> {
        Self {
            poll: Poll::default(),
            controllers: [None; ResourceId::ADAPTER_ID_MAX],
            processors: [None; ResourceId::ADAPTER_ID_MAX],
        }
    }
}

impl<C> AdapterLauncher<C>
where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static
{
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter<C> + 'static) {
        let index = adapter_id as usize;
        assert!(
            self.controllers[index].is_none(),
            "Adapter with id {} already initialized ",
            adapter_id
        );

        let (controller, processor) = adapter.split(self.poll.create_register(adapter_id));
        self.controllers[index] = Some(Box::new(controller));
        self.processors[index] = Some(Box::new(processor));
    }

    fn launch(
        self,
    ) -> (
        Poll,
        [Option<Box<dyn Controller + Send>>; ResourceId::ADAPTER_ID_MAX],
        [Option<Box<dyn Processor<C> + Send>>; ResourceId::ADAPTER_ID_MAX],
    ) {
        (self.poll, self.controllers, self.processors)
    }
}

pub struct NetworkEngine {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    controllers: [Option<Box<dyn Controller + Send>>; ResourceId::ADAPTER_ID_MAX],
}

impl NetworkEngine {
    const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
    const ADAPTER_ALREADY_CONFIGURED: &'static str = "The adapter was not configured";

    pub fn new<C>(launcher: AdapterLauncher<C>, mut event_callback: C) -> Self
    where C: FnMut(Endpoint, AdapterEvent<'_>) + Send + 'static {
        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let (mut poll, controllers, mut processors) = launcher.launch();

        let thread = thread::Builder::new()
            .name("message-io: tcp-adapter".into())
            .spawn(move || {
                let timeout = Some(Duration::from_millis(Self::NETWORK_SAMPLING_TIMEOUT));

                while running.load(Ordering::Relaxed) {
                    poll.process_event(timeout, &mut |resource_id: ResourceId| {
                        let index = resource_id.adapter_id() as usize;
                        let processor = processors[index].as_mut().unwrap();

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
        self.controllers[adapter_id as usize]
            .as_mut()
            .expect(Self::ADAPTER_ALREADY_CONFIGURED)
            .connect(addr)
    }

    pub fn listen(
        &mut self,
        adapter_id: u8,
        addr: SocketAddr,
    ) -> io::Result<(ResourceId, SocketAddr)>
    {
        self.controllers[adapter_id as usize]
            .as_mut()
            .expect(Self::ADAPTER_ALREADY_CONFIGURED)
            .listen(addr)
    }

    pub fn remove(&mut self, id: ResourceId) -> Option<()> {
        self.controllers[id.adapter_id() as usize]
            .as_mut()
            .expect(Self::ADAPTER_ALREADY_CONFIGURED)
            .remove(id)
    }

    pub fn local_addr(&self, id: ResourceId) -> Option<SocketAddr> {
        self.controllers[id.adapter_id() as usize]
            .as_ref()
            .expect(Self::ADAPTER_ALREADY_CONFIGURED)
            .local_addr(id)
    }

    pub fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus {
        self.controllers[endpoint.resource_id().adapter_id() as usize]
            .as_mut()
            .expect(Self::ADAPTER_ALREADY_CONFIGURED)
            .send(endpoint, data)
    }
}

impl Drop for NetworkEngine {
    fn drop(&mut self) {
        self.thread_running.store(false, Ordering::Relaxed);
        self.thread.take().unwrap().join().expect(OTHER_THREAD_ERR);
    }
}
