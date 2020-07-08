use crate::events::{EventSender, Event};
use crate::network::{self, Connection};

use serde::{Serialize, Deserialize};

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::net::{SocketAddr};
use std::thread::{self, JoinHandle};
use std::fmt::{self};
use std::time::{Duration};

pub type ConnectionId = usize;

const NETWORK_SAMPLING_TIMEOUT: u64 = 50; //ms

#[derive(Debug, Clone, Copy)]
pub enum TransportProtocol {
    Tcp,
    Udp,
}

impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

pub struct NetworkManager {
    network_event_thread: Option<JoinHandle<()>>,
    network_thread_running: Arc<AtomicBool>,
    network_controller: network::Controller,
    output_buffer: Vec<u8>,
}

impl<'a> NetworkManager {
    pub fn new<M, S>(event_sender: EventSender<Event<M, S, ConnectionId>>) -> NetworkManager
    where M: Serialize + for<'b> Deserialize<'b> + Send + 'static, S: Send + 'static {
        let (network_controller, mut network_receiver) = network::adapter();

        let network_thread_running = Arc::new(AtomicBool::new(true));
        let running = network_thread_running.clone();

        let network_event_thread = thread::spawn(move || {
            let timeout = Duration::from_millis(NETWORK_SAMPLING_TIMEOUT);
            while running.load(Ordering::Relaxed) {
                network_receiver.receive(Some(timeout), |connection_id, event| {
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
                });
            }
        });

        NetworkManager {
            network_event_thread: Some(network_event_thread),
            network_thread_running,
            network_controller,
            output_buffer: Vec::new()
        }
    }

    pub fn connect(&mut self, addr: SocketAddr, transport: TransportProtocol) -> Option<ConnectionId> {
        match transport {
            TransportProtocol::Tcp => Connection::new_tcp_stream(addr),
            TransportProtocol::Udp => Connection::new_udp_socket(addr),
        }
        .ok()
        .map(|connection| self.network_controller.add_connection(connection))
    }

    pub fn listen(&mut self, addr: SocketAddr, transport: TransportProtocol) -> Option<ConnectionId> {
        match transport {
            TransportProtocol::Tcp => Connection::new_tcp_listener(addr),
            TransportProtocol::Udp => Connection::new_udp_listener(addr),
        }
        .ok()
        .map(|connection| self.network_controller.add_connection(connection))
    }

    pub fn connection_address(&mut self, connection_id: ConnectionId) -> Option<SocketAddr> {
        self.network_controller.connection_address(connection_id)
    }

    pub fn remove_connection(&mut self, connection_id: ConnectionId) {
        self.network_controller.remove_connection(connection_id)
    }

    pub fn send<M>(&mut self, connection_id: ConnectionId, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        self.network_controller.send(connection_id, &self.output_buffer);
        self.output_buffer.clear();
    }

    pub fn send_all<'b, M>(&mut self, connection_ids: impl IntoIterator<Item=&'b ConnectionId>, message: M)
    where M: Serialize + Deserialize<'a> + Send + 'static {
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        for id in connection_ids {
            self.network_controller.send(*id, &self.output_buffer);
        }
        self.output_buffer.clear();
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.network_thread_running.store(false, Ordering::Relaxed);
        self.network_event_thread.take().unwrap().join().unwrap();
    }
}
