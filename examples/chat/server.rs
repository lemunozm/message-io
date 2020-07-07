use super::common::Message;

use message_io::events::{EventQueue, Event};
use message_io::network_manager::{NetworkManager, ConnectionId};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::collections::{HashMap};

enum Signal {
    Close,
    NotifyDisconnection,
}

pub fn run() {
    let mut event_queue = EventQueue::new();
    let mut network = NetworkManager::new(event_queue.sender().clone());

    let clients: HashMap<ConnectionId, SocketAddr> = HashMap::new();

    let addr = "127.0.0.1:3000".parse().unwrap();
    if let Some(server) = network.create_tcp_listener(addr) {
        println!("Server running at {}", addr);
        event_queue.sender().send_with_timer(Event::Signal(Signal::NotifyDisconnection), Duration::from_secs(5));

        loop {
            match event_queue.receive() {
                Event::Signal(signal) => match signal {
                    Signal::NotifyDisconnection => {
                        let disconnection_time = Duration::from_secs(3);
                        println!("The server will be disconnected in {} secs", disconnection_time.as_secs());
                        network.send_all(clients.keys(), Message::Info(String::from("This is client info")));
                        event_queue.sender().send_with_timer(Event::Signal(Signal::Close), disconnection_time);
                    },
                    Signal::Close => {
                        println!("Closing server");
                        network.send_all(clients.keys(), Message::Bye);
                        for endpoint in clients.keys() {
                            network.remove_connection(*endpoint);
                        }
                        return;
                    }
                }
                Event::Message(message, endpoint) => match message {
                    Message::Info(text) => println!("Client: {} says: {}", clients[&endpoint], text),
                    Message::Bye => println!("Client {} closed", clients[&endpoint]),
                    _ => eprintln!("Unexpected message from {}", clients[&endpoint]),
                },
                Event::AddedEndpoint(endpoint) => {
                    println!("Client {} connected (total clients: {})", clients[&endpoint], clients.len());
                }
                Event::RemovedEndpoint(endpoint) => {
                    println!("Client {} disconnected (total clients: {})", clients[&endpoint], clients.len());
                }
                _ => unreachable!()
            }
        }
    }
    else {
        println!("Can not listening at selected address/port");
    }
}
