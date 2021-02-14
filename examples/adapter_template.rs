#![allow(unused_variables)]

use message_io::remote_addr::{RemoteAddr};
use message_io::adapter::{
    Resource, Adapter, ActionHandler, EventHandler, SendStatus, ReadStatus, AcceptedType,
};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

pub struct ClientResource;
impl Resource for ClientResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

pub struct ServerResource;
impl Resource for ServerResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

pub struct MyAdapter;
impl Adapter for MyAdapter {
    type Remote = ClientResource;
    type Listener = ServerResource;
    type ActionHandler = MyActionHandler;
    type EventHandler = MyEventHandler;

    fn split(self) -> (MyActionHandler, MyEventHandler) {
        todo!();
    }
}

pub struct MyActionHandler;
impl ActionHandler for MyActionHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn connect(&mut self, remote_addr: RemoteAddr) -> io::Result<(ClientResource, SocketAddr)> {
        todo!();
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(ServerResource, SocketAddr)> {
        todo!();
    }

    fn send(&mut self, resource: &ClientResource, data: &[u8]) -> SendStatus {
        todo!();
    }
}

pub struct MyEventHandler;
impl EventHandler for MyEventHandler {
    type Remote = ClientResource;
    type Listener = ServerResource;

    fn accept_event(
        &mut self,
        listener: &Self::Listener,
        accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>),
    )
    {
        todo!();
    }

    fn read_event(&mut self, stream: &ClientResource, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        todo!();
    }
}
