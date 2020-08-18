use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent, Endpoint};

use std::net::{SocketAddr};
use std::collections::{HashMap};

enum Event {
    Network(NetEvent<Message>),
}

pub fn run() {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let mut clients: HashMap<Endpoint, SocketAddr> = HashMap::new();

    let listen_addr = "127.0.0.1:3000";
    match network.listen_tcp(listen_addr) {
        Some(_) => println!("Tcp Server running by TCP at {}", listen_addr),
        None => return println!("Can not listening at selected interface/port"),
    }

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    Message::Greetings(text) => {
                        println!("Client ({}) says: {}", clients[&endpoint], text);
                        network.send(endpoint, Message::Greetings("Hi, I hear you".into()));
                    },
                },
                NetEvent::AddedEndpoint(endpoint, addr) => {
                    clients.insert(endpoint, addr);
                    println!("Client ({}) connected (total clients: {})", addr, clients.len());
                },
                NetEvent::RemovedEndpoint(endpoint) => {
                    let addr = clients.remove(&endpoint).unwrap();
                    println!("Client ({}) disconnected (total clients: {})", addr, clients.len());
                }
            },
        }
    }
}
