use super::endpoint::{Endpoint};
use super::resource_id::{ResourceId};
use super::poll::{Poll};
use super::remote_addr::{RemoteAddr};
use super::driver::{NetEvent, Driver, ActionController, EventProcessor};
use super::adapter::{Adapter, SendStatus};

use std::net::{SocketAddr};
use std::io::{self};

pub type ActionControllerList = Vec<Box<dyn ActionController + Send>>;
pub type EventProcessorList = Vec<Box<dyn EventProcessor + Send>>;

/// Used to configured the engine
pub struct AdapterLoader {
    poll: Poll,
    controllers: ActionControllerList,
    processors: EventProcessorList,
}

impl Default for AdapterLoader {
    fn default() -> AdapterLoader {
        Self {
            poll: Poll::default(),
            controllers: (0..ResourceId::MAX_ADAPTERS)
                .map(|_| Box::new(UnimplementedDriver) as Box<dyn ActionController + Send>)
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::MAX_ADAPTERS)
                .map(|_| Box::new(UnimplementedDriver) as Box<dyn EventProcessor + Send>)
                .collect(),
        }
    }
}

impl AdapterLoader {
    /// Mount an adapter associating it with an id.
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter + 'static) {
        let index = adapter_id as usize;

        let driver = Driver::new(adapter, adapter_id, &mut self.poll);

        self.controllers[index] = Box::new(driver.clone()) as Box<(dyn ActionController + Send)>;
        self.processors[index] = Box::new(driver) as Box<(dyn EventProcessor + Send)>;
    }

    /// Consume this instance to obtain the driver handles.
    pub fn take(self) -> (Poll, ActionControllerList, EventProcessorList) {
        (self.poll, self.controllers, self.processors)
    }
}

// The following unimplemented driver is used to fill
// the invalid adapter id holes in the controllers / processors.
// It is faster and cleanest than to use an option that always must to be unwrapped.

const UNIMPLEMENTED_DRIVER_ERR: &str =
    "The chosen adapter id doesn't reference an existing adapter";

struct UnimplementedDriver;
impl ActionController for UnimplementedDriver {
    fn connect(&self, _: RemoteAddr) -> io::Result<(Endpoint, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn listen(&self, _: SocketAddr) -> io::Result<(ResourceId, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn send(&self, _: Endpoint, _: &[u8]) -> SendStatus {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn remove(&self, _: ResourceId) -> bool {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }
}

impl EventProcessor for UnimplementedDriver {
    fn process(&self, _: ResourceId, _: &mut dyn FnMut(NetEvent<'_>)) {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }
}
