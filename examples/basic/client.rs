use super::common::{ClientMessage, ServerMessage};

use message_io::events::{EventQueue, Event};
use message_io::network_manager::{NetworkManager, TransportProtocol};

use std::time::{Duration};

enum Signal {
    Greet
}

pub fn run(protocol: TransportProtocol) {
    let mut event_queue = EventQueue::new();
    let mut network = NetworkManager::new(event_queue.sender().clone());

    let addr = "127.0.0.1:3000".parse().unwrap();
    if let Some(server) = network.connect(addr, protocol) {
        println!("Connected to server by {} at {}", protocol, addr);
        event_queue.sender().send_with_timer(Event::Signal(Signal::Greet), Duration::from_secs(1));

        let mut hello_counter = 0;
        loop {
            match event_queue.receive() {
                Event::Signal(signal) => match signal {
                    Signal::Greet => {
                        println!("Saying hello to the server... ({})", hello_counter);
                        network.send(server, ClientMessage::Greet(format!("Hello ({})", hello_counter)));
                        event_queue.sender().send_with_timer(Event::Signal(Signal::Greet), Duration::from_secs(2));
                        hello_counter += 1;
                    },
                }
                Event::Message(message, _) => match message {
                    ServerMessage::Greet(text) => println!("Server says: {}", text),
                    ServerMessage::NotifyDisconnection(duration) => println!("Server notified disconnection in {} secs", duration.as_secs()),
                    ServerMessage::Bye => println!("Server say: good bye!"),
                },
                Event::AddedEndpoint(_) => unreachable!(),
                Event::RemovedEndpoint(_) => {
                    println!("Server is disconnected");
                    return;
                }
            }
        }
    }
    else {
        println!("Can not connect to the server by {} to {}", protocol, addr);
    }
}
