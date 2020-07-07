use crate::events::{EventSender, Event};
use crate::network::{self, Connection};

use serde::{Serialize, Deserialize};

use std::net::{SocketAddr};
use std::thread::{self, JoinHandle};

pub type ConnectionId = usize;

pub struct NetworkManager {
    network_event_thread: JoinHandle<()>,
    network_controller: network::Controller,
}

impl<'a> NetworkManager {
    pub fn new<M, S>(mut event_sender: EventSender<Event<M, S, ConnectionId>>) -> NetworkManager
    where M: Serialize + Deserialize<'a> + Send + 'static, S: Send + 'static {
        let (network_controller, mut network_receiver) = network::adapter();

        let network_event_thread = thread::spawn(move || {
            loop {
                let (connection_id, event) = network_receiver.receive();
                match event {
                    network::Event::Connection => {
                        event_sender.send(Event::AddedEndpoint(connection_id));
                    }
                    network::Event::Data(data) => {
                        let message: M = bincode::deserialize(&data[..]).unwrap();
                        event_sender.send(Event::Message(message, connection_id));
                    }
                    network::Event::Disconnection => {
                        event_sender.send(Event::RemovedEndpoint(connection_id));
                    }
                }
            }
        });

        NetworkManager {
            network_event_thread,
            network_controller,
        }
    }

    pub fn create_tcp_stream(&mut self, addr: SocketAddr) -> Option<ConnectionId> {
        Connection::new_tcp_stream(addr)
            .ok()
            .map(|connection| self.network_controller.add_connection(connection))
    }

    pub fn create_tcp_listener(&mut self, addr: SocketAddr) -> Option<ConnectionId> {
        Connection::new_tcp_listener(addr)
            .ok()
            .map(|connection| self.network_controller.add_connection(connection))
    }

    pub fn remove_connection(&mut self, connection_id: ConnectionId) {
        self.network_controller.remove_connection(connection_id)
    }

    pub fn send<M>(&mut self, connection_id: ConnectionId, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
    }

    pub fn send_all<'b, M>(&mut self, connection_ids: impl IntoIterator<Item=&'b ConnectionId>, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
    }
}
