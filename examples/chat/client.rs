use super::common::Message;

use message_io::events::{self, Event};
use message_io::network_manager::{NetworkManager, Endpoint};

use std::time::{Duration};


enum Signal {
    Close,
    WriteToServer
}

pub fn run() {
    let mut event_queue = EventQueue::new<Message, Signal, Endpoint>();
    let mut network = NetworkManager::new(event_queue.sender().clone());

    if let Some(server) = network.create_tcp_connection("127.0.0.1:3000".parse().unwrap()) {
        println!("Server connected");
        event_queue.sender().send(Event::Signal(Signal::WriteToServer));

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
                        network.remove_tcp_connection(server);
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
