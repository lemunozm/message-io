use crate::events::{EventSender, Event};
use crate::network::{self, Connection};

use serde::{Serialize, Deserialize};

use std::net::{SocketAddr};
use std::thread::{self, JoinHandle};

pub type Endpoint = usize;

pub struct NetworkManager {
    network_event_thread: JoinHandle<()>,
    network_controller: network::Controller,
}

impl<'a> NetworkManager {
    pub fn new<M>(mut event_sender: EventSender<Event<M, (), Endpoint>>) -> NetworkManager
    where M: Serialize + Deserialize<'a> + Send + 'static {
        let (network_controller, mut network_receiver) = network::adapter();

        let network_event_thread = thread::spawn(move || {
            loop {
                let (endpoint, event) = network_receiver.receive();
                match event {
                    network::Event::Connection => {
                        event_sender.send(Event::AddedEndpoint(endpoint));
                    }
                    network::Event::Data(data) => {
                        let message: M = bincode::deserialize(&data[..]).unwrap();
                        event_sender.send(Event::Message(message, endpoint));
                    }
                    network::Event::Disconnection => {
                        event_sender.send(Event::RemovedEndpoint(endpoint));
                    }
                }
            }
        });

        NetworkManager {
            network_event_thread,
            network_controller,
        }
    }

    pub fn create_tcp_stream(&mut self, addr: SocketAddr) -> Option<Endpoint> {
        Connection::new_tcp_stream(addr).ok().map(|connection| {
            self.network_controller.add_endpoint(connection)
        })
    }

    pub fn create_tcp_listener(&mut self, addr: SocketAddr) -> Option<Endpoint> {
        Connection::new_tcp_listener(addr).ok().map(|connection| {
            self.network_controller.add_endpoint(connection)
        })
    }

    pub fn remove_tcp_listener(&mut self, endpoint: Endpoint) {
        self.network_controller.remove_endpoint(endpoint)
    }

    pub fn send<M>(&mut self, endpoint: Endpoint, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
        todo!()
    }

    pub fn send_all<M>(&mut self, endpoint: Vec<Endpoint>, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
        todo!()
    }
}
