use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent, TransportProtocol};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum Message {
    HelloServer,
    HelloClient,
    // Other messages here
}

enum Event {
    Network(NetEvent<Message>),
    // Other user events here
}

fn main() {
    let mut event_queue = EventQueue::new();

    // Create NetworkManager, the callback will push the network event into the event queue
    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(Event::Network(net_event)));

    // Listen TCP messages at ports 3001 and 3002.
    network.listen("127.0.0.1:3001".parse().unwrap(), TransportProtocol::Tcp).unwrap();
    network.listen("0.0.0.0.0:3002".parse().unwrap(), TransportProtocol::Tcp).unwrap();

    loop {
        match event_queue.receive() { // Read the next event or wait until have it.
            Event::Network(net_event) => match net_event {
                NetEvent::Message(message, endpoint) => match message {
                    Message::HelloServer => network.send(endpoint, Message::HelloClient).unwrap(),
                    _ => (), // Other messages here
                },
                NetEvent::AddedEndpoint(_endpoint, _address) => println!("Client connected"),
                NetEvent::RemovedEndpoint(_endpoint) => println!("Client disconnected"),
            },
            // Other events here
        }
    }
}
