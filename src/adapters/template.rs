#![allow(unused_variables)]

use crate::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo,
};
use crate::network::{RemoteAddr};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

pub struct MyAdapter;
impl Adapter for MyAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub struct RemoteResource;
impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

impl Remote for RemoteResource {
    fn connect(remote_addr: RemoteAddr) -> io::Result<ConnectionInfo<Self>> {
        todo!();
    }

    fn receive(&self, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        todo!();
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        todo!();
    }
}

pub struct LocalResource;
impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!();
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen(addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        todo!();
    }

    fn accept(&self, accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>)) {
        todo!();
    }
}
