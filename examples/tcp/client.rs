use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Transport};

use std::time::{Duration};

enum Event {
    Network(NetEvent<Message>),
    Greet,
}

pub fn run(name: &str) {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let server_addr = "127.0.0.1:3000";
    if let Ok(server_id) = network.connect(Transport::Tcp, server_addr) {
        println!("Connect to server by TCP at {}", server_addr);
        event_queue.sender().send(Event::Greet);

        loop {
            match event_queue.receive() {
                Event::Greet => {
                    network.send(server_id, Message::Greetings(format!("Hi, I am {}", name)));
                    event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(1));
                }
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(_, message) => match message {
                        Message::Greetings(text) => println!("Server says: {}", text),
                    },
                    NetEvent::AddedEndpoint(_) => unreachable!(),
                    NetEvent::RemovedEndpoint(_) => {
                        println!("Server is disconnected");
                        return
                    }
                    NetEvent::DeserializationError(_) => (),
                },
            }
        }
    }
    else {
        println!("Can not connect to the server by TCP to {}", server_addr);
    }
}
