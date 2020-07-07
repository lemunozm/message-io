use super::common::Message;

use message_io::events::{EventQueue, Event};
use message_io::network_manager::{NetworkManager, TransportProtocol};

use std::time::{Duration};


enum Signal {
    Close,
    WriteToServer
}

pub fn run() {
    let mut event_queue = EventQueue::new();
    let mut network = NetworkManager::new(event_queue.sender().clone());

    let addr = "127.0.0.1:3000".parse().unwrap();
    if let Some(server) = network.connect(addr, TransportProtocol::Tcp) {
        println!("Connected to server: {}", addr);
        event_queue.sender().send_with_timer(Event::Signal(Signal::WriteToServer), Duration::from_secs(1));

        loop {
            match event_queue.receive() {
                Event::Signal(signal) => match signal {
                    Signal::WriteToServer => {
                        println!("Sending info to the server");
                        network.send(server, Message::Info(String::from("This is client info")));
                        event_queue.sender().send_with_timer(Event::Signal(Signal::WriteToServer), Duration::from_secs(2));
                    },
                    Signal::Close => {
                        println!("Closing client");
                        network.send(server, Message::Bye);
                        network.remove_connection(server);
                        return;
                    }
                }
                Event::Message(message, _) => match message {
                    Message::Info(text) => println!("Server says: {}", text),
                    Message::NotifyDisconnection(duration) => println!("Server will be disconnected in {} secs", duration.as_secs()),
                    Message::Bye => println!("Server is closing"),
                },
                Event::RemovedEndpoint(_) => {
                    println!("Server is disconnected");
                    return;
                }
                _ => unreachable!()
            }
        }
    }
    else {
        println!("Can not connect to the server");
    }
}
