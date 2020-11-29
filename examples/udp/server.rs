use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent};

enum Event {
    Network(NetEvent<Message>),
}

pub fn run() {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let listen_addr = "127.0.0.1:3000";
    match network.listen_udp(listen_addr) {
        Ok(_) => println!("UDP Server running at {}", listen_addr),
        Err(_) => return println!("Can not listening at {}", listen_addr),
    }

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    Message::Greetings(text) => {
                        println!("Client ({}) says: {}", endpoint.addr(), text);
                        network.send(endpoint, Message::Greetings("Hi, I hear you".into()));
                    }
                },
                NetEvent::AddedEndpoint(_) => unreachable!(), // Not be generated for UDP
                NetEvent::RemovedEndpoint(_) => unreachable!(), // Not be generated for UDP
                NetEvent::DeserializationError(_) => (),
            },
        }
    }
}
