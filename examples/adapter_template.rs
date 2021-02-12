#![allow(unused_variables)]

use message_io::adapter::{Adapter, ActionHandler, EventHandler, SendStatus, ReadStatus, AcceptedType};

use std::net::{SocketAddr};
use std::io::{self};

use resource::{MyRemote, MyListener};

pub struct MyAdapter;
impl Adapter for MyAdapter {
    type Remote = MyRemote;
    type Listener = MyListener;
    type ActionHandler = MyActionHandler;
    type EventHandler = MyEventHandler;

    fn split(self) -> (MyActionHandler, MyEventHandler) {
        todo!();
    }
}

pub struct MyActionHandler;
impl ActionHandler for MyActionHandler {
    type Remote = resource::MyRemote;
    type Listener = resource::MyListener;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<MyRemote> {
        todo!();
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(MyListener, SocketAddr)> {
        todo!();
    }

    fn send(&mut self, stream: &MyRemote, data: &[u8]) -> SendStatus {
        todo!();
    }
}

pub struct MyEventHandler;
impl EventHandler for MyEventHandler {
    type Remote = resource::MyRemote;
    type Listener = resource::MyListener;

    fn accept_event(
        &mut self,
        listener: &Self::Listener,
        accept_remote: &dyn Fn(AcceptedType<'_, Self::Remote>),
    )
    {
        todo!();
    }

    fn read_event(&mut self, stream: &MyRemote, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        todo!();
    }
}

// You NOT need to create this module if your Remote/Listener already implements Source

mod resource {

    use mio::{
        event::{Source},
        Interest, Token, Registry,
    };
    use std::io::{self};

    pub struct MyRemote;
    impl Source for MyRemote {
        fn register(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()>
        {
            todo!();
        }

        fn reregister(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()>
        {
            todo!();
        }

        fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
            todo!();
        }
    }

    pub struct MyListener;
    impl Source for MyListener {
        fn register(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()>
        {
            todo!();
        }

        fn reregister(
            &mut self,
            registry: &Registry,
            token: Token,
            interests: Interest,
        ) -> io::Result<()>
        {
            todo!();
        }

        fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
            todo!();
        }
    }
}
