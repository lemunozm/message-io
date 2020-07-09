use super::common::{ClientMessage, ServerMessage};

use message_io::event_queue::{EventQueue};
use message_io::network_manager::{NetworkManager, NetEvent, TransportProtocol};

use std::time::{Duration};

enum Event {
    Network(NetEvent<ServerMessage>),
    Greet,
}

pub fn run(protocol: TransportProtocol) {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let addr = "127.0.0.1:3000".parse().unwrap();
    if let Some(server) = network.connect(addr, protocol) {
        println!("Connected to server by {} at {}", protocol, addr);
        event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(1));

        let mut hello_counter = 0;
        loop {
            match event_queue.receive() {
                Event::Greet => {
                    println!("Saying hello to the server... ({})", hello_counter);
                    network.send(server, ClientMessage::Greet(format!("Hello ({})", hello_counter)));
                    event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(2));
                    hello_counter += 1;
                }
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(message, _) => match message {
                        ServerMessage::Greet(text) => println!("Server says: {}", text),
                        ServerMessage::NotifyDisconnection(duration) => println!("Server notified disconnection in {} secs", duration.as_secs()),
                        ServerMessage::Bye => println!("Server say: good bye!"),
                    },
                    NetEvent::AddedEndpoint(_) => unreachable!(),
                    NetEvent::RemovedEndpoint(_) => {
                        println!("Server is disconnected");
                        return;
                    }
                }
            }
        }
    }
    else {
        println!("Can not connect to the server by {} to {}", protocol, addr);
    }
}
