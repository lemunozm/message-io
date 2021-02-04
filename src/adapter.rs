use crate::endpoint::{Endpoint};
use crate::resource_id::{ResourceId};
use crate::poll::{PollRegister};
use crate::util::{SendingStatus};

use std::net::{SocketAddr};
use std::io::{self};

pub enum AdapterEvent<'a> {
    Added,
    Data(&'a [u8]),
    Removed,
}

pub trait Adapter<C>
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    type Controller: Controller + Send;
    type Processor: Processor<C> + Send;

    fn split(&self, poll_register: PollRegister) -> (Self::Controller, Self::Processor);
}

pub trait Controller {
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Endpoint>;
    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ResourceId, SocketAddr)>;
    fn remove(&mut self, id: ResourceId) -> Option<()>;
    fn local_addr(&self, id: ResourceId) -> Option<SocketAddr>;
    fn send(&mut self, endpoint: Endpoint, data: &[u8]) -> SendingStatus;
}

pub trait Processor<C>
where C: FnMut(Endpoint, AdapterEvent<'_>)
{
    fn process_listener(&mut self, id: ResourceId, event_callback: &mut C);
    fn process_remote(&mut self, id: ResourceId, event_callback: &mut C);
}
