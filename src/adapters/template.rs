#![allow(unused_variables)]

use crate::adapter::{Resource, Remote, Listener, Adapter, SendStatus, AcceptedType, ReadStatus};
use crate::remote_addr::{RemoteAddr};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

pub struct MyAdapter;
impl Adapter for MyAdapter {
    type Remote = RemoteResource;
    type Listener = ListenerResource;
}

pub struct RemoteResource;
impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<(Self, SocketAddr)> {
        todo!();
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        todo!();
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        todo!();
    }
}

pub struct ListenerResource;
impl Resource for ListenerResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

impl Listener for ListenerResource {
    type Remote = RemoteResource;

    fn listen(addr: SocketAddr) -> io::Result<(Self, SocketAddr)> {
        todo!();
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        todo!();
    }
}
