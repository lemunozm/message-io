use crate::events::{InputEventHandle};

use std::net::SocketAddr;

pub type Endpoint = usize;

pub struct NetworkManager<M> {
    input_event_handle: InputEventHandle<M, Endpoint>
    //network_adapter
}

impl<M> NetworkManager<M> {
    pub fn new(input_event_handle: InputEventHandle<M, Endpoint>) -> NetworkManager<M> {
        NetworkManager {
            input_event_handle
        }
    }

    pub fn create_tcp_connection(&mut self, addr: SocketAddr) -> Option<Endpoint> {
        Some(0)
    }

    pub fn create_tcp_listener(&mut self, add: SocketAddr) -> Option<Endpoint> {
        Some(0)
    }

    pub fn remove_tcp_connection(&mut self, endpoint: Endpoint) {

    }

    pub fn remove_tcp_listener(&mut self, endpoint: Endpoint) {

    }

    pub fn send(&mut self, endpoint: Endpoint, message: M) {

    }

    pub fn send_all(&mut self, endpoint: Vec<Endpoint>, message: M) {

    }
}
