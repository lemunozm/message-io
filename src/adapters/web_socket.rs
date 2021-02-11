use crate::adapter::{Adapter, ActionHandler, EventHandler};
use crate::status::{SendStatus, AcceptStatus, ReadStatus};

use std::net::{SocketAddr};
use std::io::{self};

mod resource {
    use mio::{event::{Source}, Interest, Token, Registry};
    use std::io::{self};

    pub struct WsRemote;
    impl Source for WsRemote {
        fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
            -> io::Result<()>
        {
            todo!();
        }

        fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
            -> io::Result<()>
        {
            todo!();
        }

        fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
            todo!();
        }
    }

    pub struct WsListener;
    impl Source for WsListener {
        fn register(&mut self, registry: &Registry, token: Token, interests: Interest)
            -> io::Result<()>
        {
            todo!();
        }

        fn reregister(&mut self, registry: &Registry, token: Token, interests: Interest)
            -> io::Result<()>
        {
            todo!();
        }

        fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
            todo!();
        }
    }
}

use resource::{WsRemote, WsListener};

pub struct WsAdapter;
impl Adapter for WsAdapter {
    type Remote = WsRemote;
    type Listener = WsListener;
    type ActionHandler = WsActionHandler;
    type EventHandler = WsEventHandler;

    fn split(self) -> (WsActionHandler, WsEventHandler) {
        todo!();
    }
}

pub struct WsActionHandler;
impl ActionHandler for WsActionHandler {
    type Remote = resource::WsRemote;
    type Listener = resource::WsListener;

    fn connect(&mut self, addr: SocketAddr) -> io::Result<WsRemote> {
        todo!();
    }

    fn listen(&mut self, addr: SocketAddr) -> io::Result<(WsListener, SocketAddr)> {
        todo!();
    }

    fn send(&mut self, stream: &WsRemote, data: &[u8]) -> SendStatus {
        todo!();
    }
}

pub struct WsEventHandler;
impl EventHandler for WsEventHandler {
    type Remote = resource::WsRemote;
    type Listener = resource::WsListener;

    fn accept_event(&mut self, listener: &Self::Listener) -> AcceptStatus<'_, Self::Remote> {
        todo!();
    }

    fn read_event(&mut self, stream: &WsRemote, process_data: &dyn Fn(&[u8])) -> ReadStatus {
        todo!();
    }
}
