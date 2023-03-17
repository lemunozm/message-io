#![allow(unused_variables)]

use crate::network::adapter::{
    Resource, Remote, Local, Adapter, SendStatus, AcceptedType, ReadStatus, ConnectionInfo,
    ListeningInfo, PendingStatus,
};
use crate::network::{RemoteAddr, Readiness, TransportConnect, TransportListen};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

pub(crate) struct MyAdapter;
impl Adapter for MyAdapter {
    type Remote = RemoteResource;
    type Local = LocalResource;
}

pub(crate) struct RemoteResource;
impl Resource for RemoteResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!()
    }
}

impl Remote for RemoteResource {
    fn connect_with(
        config: TransportConnect,
        remote_addr: RemoteAddr,
    ) -> io::Result<ConnectionInfo<Self>> {
        todo!()
    }

    fn receive(&self, process_data: impl FnMut(&[u8])) -> ReadStatus {
        todo!()
    }

    fn send(&self, data: &[u8]) -> SendStatus {
        todo!()
    }

    fn pending(&self, _readiness: Readiness) -> PendingStatus {
        todo!()
    }
}

pub(crate) struct LocalResource;
impl Resource for LocalResource {
    fn source(&mut self) -> &mut dyn Source {
        todo!()
    }
}

impl Local for LocalResource {
    type Remote = RemoteResource;

    fn listen_with(config: TransportListen, addr: SocketAddr) -> io::Result<ListeningInfo<Self>> {
        todo!()
    }

    fn accept(&self, accept_remote: impl FnMut(AcceptedType<'_, Self::Remote>)) {
        todo!()
    }
}
