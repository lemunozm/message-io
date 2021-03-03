use crate::adapter::{TransportAdapter, Endpoint, AdapterEvent};
use crate::resource_id::{ResourceId, ResourceType, ResourceIdGenerator};
use crate::util::{OTHER_THREAD_ERR};

use mio::net::{UdpSocket};
use mio::{Poll, Interest, Token, Events, Registry};

use std::net::{SocketAddr};
use std::time::{Duration};
use std::collections::{HashMap};
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Deref};

pub const MAX_UDP_LEN: usize = 1488;
const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms
const EVENTS_SIZE: usize = 1024;

pub struct UdpAdapter {
    thread: Option<JoinHandle<()>>,
    thread_running: Arc<AtomicBool>,
    store: Arc<Store>,
}

struct Store {
    //TODO
}

impl TransportAdapter for UdpAdapter {
    type Listener = UdpSocket;
    type Remote = UdpSocket;

    fn init<C>(id_generator: ResourceIdGenerator, mut event_callback: C) -> Self where
    C: for<'b> FnMut(Endpoint, AdapterEvent<'b>) + Send + 'static {

        let poll = Poll::new().unwrap();
        let store = Store{}; //TODO
        let store = Arc::new(store);
        let thread_store = store.clone();

        let thread_running = Arc::new(AtomicBool::new(true));
        let running = thread_running.clone();

        let thread = thread::Builder::new()
            .name("message-io: udp-adapter".into())
            .spawn(move || {
                let mut input_buffer = [0; MAX_UDP_LEN];
                let timeout = Some(Duration::from_millis(NETWORK_SAMPLING_TIMEOUT));
                while running.load(Ordering::Relaxed) {
                    todo!()
                }
            })
            .unwrap();

        Self {
            thread: Some(thread),
            thread_running,
            store,
        }
    }

    fn add_listener(&mut self, mut listener: UdpSocket) -> (ResourceId, SocketAddr) {
        todo!()
    }

    fn add_remote(&mut self, mut remote: UdpSocket) -> Endpoint {
        todo!()
    }

    fn remove_listener(&mut self, id: ResourceId) -> Option<()> {
        todo!()
    }

    fn remove_remote(&mut self, id: ResourceId) -> Option<()> {
        todo!()
    }

    fn local_address(&self, id: ResourceId) -> Option<SocketAddr> {
        todo!()
    }

    fn send(&mut self, endpoint: Endpoint, data: &[u8]) {
        todo!()
    }
}

impl Drop for UdpAdapter {
    fn drop(&mut self) {
        self.thread_running.store(false, Ordering::Relaxed);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect(OTHER_THREAD_ERR);
    }
}
