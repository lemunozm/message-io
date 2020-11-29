use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Endpoint};

use std::collections::{HashMap};

struct ClientInfo {
    count: usize,
}

enum Event {
    Network(NetEvent<Message>),
}

pub fn run() {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let mut clients: HashMap<Endpoint, ClientInfo> = HashMap::new();

    let listen_addr = "127.0.0.1:3000";
    match network.listen_tcp(listen_addr) {
        Ok(_) => println!("TCP Server running at {}", listen_addr),
        Err(_) => return println!("Can not listening at {}", listen_addr),
    }

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    Message::Greetings(text) => {
                        let mut client_info = clients.get_mut(&endpoint).unwrap();
                        client_info.count += 1;
                        println!(
                            "Client ({}) says '{}' {} times",
                            endpoint.addr(),
                            text,
                            client_info.count
                        );
                        let msg = format!("Hi, I hear you for {} time", client_info.count);
                        network.send(endpoint, Message::Greetings(msg));
                    }
                },
                NetEvent::AddedEndpoint(endpoint) => {
                    clients.insert(endpoint, ClientInfo { count: 0 });
                    println!(
                        "Client ({}) connected (total clients: {})",
                        endpoint.addr(),
                        clients.len()
                    );
                }
                NetEvent::RemovedEndpoint(endpoint) => {
                    clients.remove(&endpoint).unwrap();
                    println!(
                        "Client ({}) disconnected (total clients: {})",
                        endpoint.addr(),
                        clients.len()
                    );
                }
                NetEvent::DeserializationError(_) => (),
            },
        }
    }
}
