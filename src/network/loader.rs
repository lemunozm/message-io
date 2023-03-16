use crate::network::{TransportConnect, TransportListen};

use super::endpoint::{Endpoint};
use super::resource_id::{ResourceId};
use super::poll::{Poll, Readiness};
use super::remote_addr::{RemoteAddr};
use super::driver::{NetEvent, Driver, ActionController, EventProcessor};
use super::adapter::{Adapter, SendStatus};

use std::net::{SocketAddr};
use std::io::{self};
use std::panic::{UnwindSafe};

type Controller = Box<dyn ActionController + Send + UnwindSafe>;
type Processor = Box<dyn EventProcessor + Send + UnwindSafe>;

pub type ActionControllerList = Vec<Controller>;
pub type EventProcessorList = Vec<Processor>;

/// Used to configured the engine
pub struct DriverLoader {
    poll: Poll,
    controllers: ActionControllerList,
    processors: EventProcessorList,
}

impl Default for DriverLoader {
    fn default() -> DriverLoader {
        Self {
            poll: Poll::default(),
            controllers: (0..ResourceId::MAX_ADAPTERS)
                .map(|_| Box::new(UnimplementedDriver) as Controller)
                .collect::<Vec<_>>(),
            processors: (0..ResourceId::MAX_ADAPTERS)
                .map(|_| Box::new(UnimplementedDriver) as Processor)
                .collect(),
        }
    }
}

impl DriverLoader {
    /// Mount an adapter to create its driver associating it with an id.
    pub fn mount(&mut self, adapter_id: u8, adapter: impl Adapter + 'static) {
        let index = adapter_id as usize;

        let driver = Driver::new(adapter, adapter_id, &mut self.poll);

        self.controllers[index] = Box::new(driver.clone()) as Controller;
        self.processors[index] = Box::new(driver) as Processor;
    }

    /// Consume this instance to obtain the driver handles.
    pub fn take(self) -> (Poll, ActionControllerList, EventProcessorList) {
        (self.poll, self.controllers, self.processors)
    }
}

// The following unimplemented driver is used to fill
// the invalid adapter id gaps in the controllers/processors lists.
// It is faster and cleanest than to use an option that always must to be unwrapped.

const UNIMPLEMENTED_DRIVER_ERR: &str =
    "The chosen adapter id doesn't reference an existing adapter";

struct UnimplementedDriver;
impl ActionController for UnimplementedDriver {
    fn connect_with(
        &self,
        _: TransportConnect,
        _: RemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn listen_with(
        &self,
        _: TransportListen,
        _: SocketAddr,
    ) -> io::Result<(ResourceId, SocketAddr)> {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn send(&self, _: Endpoint, _: &[u8]) -> SendStatus {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn remove(&self, _: ResourceId) -> bool {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }

    fn is_ready(&self, _: ResourceId) -> Option<bool> {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }
}

impl EventProcessor for UnimplementedDriver {
    fn process(&self, _: ResourceId, _: Readiness, _: &mut dyn FnMut(NetEvent<'_>)) {
        panic!("{}", UNIMPLEMENTED_DRIVER_ERR);
    }
}
