use std::net::{SocketAddr};

pub enum ResourceType {
    Listener,
    Remote,
}

pub type ResourceId = usize;

pub fn resource_type(id: ResourceId) -> ResourceType {
    if id & (1 << 63) != 0 {
        ResourceType::Listener
    }
    else {
        ResourceType::Remote
    }
}

pub fn stamp_resource_type(id: &mut ResourceId, resource_type: ResourceType) {
    match resource_type {
        ResourceType::Listener => *id &= 1 << 63,
        ResourceType::Remote => (),
    }
}

/// Information to identify the remote endpoint.
/// The endpoint is used mainly as a connection identified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Endpoint {
    resource_id: ResourceId,
    addr: SocketAddr,
}

/// Internal adapter event
#[derive(Debug)]
pub enum AdapterEvent<'a> {
    Connection,
    Data(&'a [u8]),
    Disconnection,
}

impl Endpoint {
    pub fn new(resource_id: ResourceId, addr: SocketAddr) -> Endpoint {
        Endpoint { resource_id, addr }
    }

    /// Returns the inner network resource id associated used for the endpoint.
    /// It is not necessary to be unique for each endpoint, if some of them shared the resource (an example of this is the different endpoints generated by the a UDP Listener).
    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    /// Returns the remote address of the endpoint
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

pub trait NetworkAdapter {
    fn init<C>(event_callback: C) -> Self where
        C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) + Send + 'static;

    fn add_remote<R>(&mut self, remote: R) -> Endpoint;
    fn add_listener<L>(&mut self, listener: L) -> (ResourceId, SocketAddr);
    fn remove_remote(&mut self, resource_id: ResourceId) -> Option<()>;
    fn remove_listener(&mut self, resource_id: ResourceId) -> Option<()>;
    fn local_address(&self, resource_id: ResourceId) -> Option<SocketAddr>;
    fn send(&mut self, endpoint: Endpoint, data: &[u8]);
}
