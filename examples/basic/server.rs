use super::common::{ClientMessage, ServerMessage};

use message_io::event_queue::{EventQueue};
use message_io::network_manager::{NetworkManager, Event, TransportProtocol, Endpoint};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::collections::{HashMap};

enum Signal {
    Close,
    NotifyDisconnection,
}

pub fn run(protocol: TransportProtocol) {
    let mut event_queue = EventQueue::new();
    let mut network = NetworkManager::new(event_queue.sender().clone());

    let mut clients: HashMap<Endpoint, SocketAddr> = HashMap::new();

    let listen_addr = "127.0.0.1:3000".parse().unwrap();
    if let Some(_) = network.listen(listen_addr, protocol) {
        println!("Server running in {} at {}", protocol, listen_addr);
        event_queue.sender().send_with_timer(Event::Signal(Signal::NotifyDisconnection), Duration::from_secs(30));

        loop {
            match event_queue.receive() {
                Event::Signal(signal) => match signal {
                    Signal::NotifyDisconnection => {
                        let disconnection_time = Duration::from_secs(5);
                        println!("The server will be disconnected in {} secs", disconnection_time.as_secs());
                        network.send_all(clients.keys(), ServerMessage::NotifyDisconnection(disconnection_time)).ok();
                        event_queue.sender().send_with_timer(Event::Signal(Signal::Close), disconnection_time);
                    },
                    Signal::Close => {
                        println!("Closing server");
                        network.send_all(clients.keys(), ServerMessage::Bye).ok();
                        for endpoint in clients.keys() {
                            network.remove_endpoint(*endpoint);
                        }
                        return;
                    }
                },
                Event::Message(message, endpoint) => match message {
                    ClientMessage::Greet(text) => {
                        println!("Client {} says: {}", clients[&endpoint], text);
                        network.send(endpoint, ServerMessage::Greet(String::from("Hi! I hear you")));
                    },
                    ClientMessage::Bye => println!("Client {} closed", clients[&endpoint]),
                },
                Event::AddedEndpoint(endpoint) => {
                    let addr = network.endpoint_address(endpoint).unwrap();
                    clients.insert(endpoint, addr);
                    println!("Client {} connected (total clients: {})", addr, clients.len());
                },
                Event::RemovedEndpoint(endpoint) => {
                    let addr = clients.remove(&endpoint).unwrap();
                    println!("Client {} disconnected (total clients: {})", addr, clients.len());
                },
            }
        }
    }
    else {
        println!("Can not listening at selected interface/port");
    }
}
