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
        Ok(_) => println!("TCP Server running at {}", listen_addr),
        Err(_) => return println!("Can not listening at selected interface/port"),
    }

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(client_id, message) => match message {
                    Message::Greetings(text) => {
                        println!("Client ({}) says: {}", clients[&client_id], text);
                        network.send(client_id, Message::Greetings("Hi, I hear you".into())).unwrap();
                    },
                },
                NetEvent::AddedEndpoint(client_id, addr) => {
                    clients.insert(client_id, addr);
                    println!("Client ({}) connected (total clients: {})", addr, clients.len());
                },
                NetEvent::RemovedEndpoint(client_id) => {
                    let addr = clients.remove(&client_id).unwrap();
                    println!("Client ({}) disconnected (total clients: {})", addr, clients.len());
                }
            },
        }
    }
}
