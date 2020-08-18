use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

enum Event {
    Network(NetEvent<Message>),
}

pub fn run() {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let listen_addr = "127.0.0.1:3000";
    match network.listen_udp(listen_addr) {
        Some(_) => println!("UDP Server running at {}", listen_addr),
        None => return println!("Can not listening at selected interface/port"),
    }

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(client_id, message) => match message {
                    Message::Greetings(text) => {
                        let addr = network.endpoint_remote_address(client_id).unwrap();
                        println!("Client ({}) says: {}", addr, text);
                        network.send(client_id, Message::Greetings("Hi, I hear you".into()));
                    },
                },
                NetEvent::AddedEndpoint(_, _) => unreachable!(), // It not be generated for UDP
                NetEvent::RemovedEndpoint(_) => unreachable!(), // It will not be generated for UDP
            },
        }
    }
}
