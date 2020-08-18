use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

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

    // Listen TCP and UDP messages on ports 3001 and 3002.
    network.listen_tcp("0.0.0.0:3001").unwrap();
    network.listen_udp("0.0.0.0:3002").unwrap();

    loop {
        match event_queue.receive() { // Read the next event or wait until have it.
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    Message::HelloServer => network.send(endpoint, Message::HelloClient).unwrap(),
                    _ => (), // Other messages here
                },
                NetEvent::AddedEndpoint(_endpoint, _address) => println!("TCP Client connected"),
                NetEvent::RemovedEndpoint(_endpoint) => println!("TCP Client disconnected"),
            },
            // Other events here
        }
    }
}
