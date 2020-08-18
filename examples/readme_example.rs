use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
enum InputMessage {
    HelloServer(String),
    // Other input messages here
}

#[derive(Serialize)]
enum OutputMessage {
    HelloClient(String),
    // Other output messages here
}

enum Event {
    Network(NetEvent<InputMessage>),
    // Other user events here
}

fn main() {
    let mut event_queue = EventQueue::new();

    // Create NetworkManager, the callback will push the network event into the event queue
    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(Event::Network(net_event)));

    // Listen from TCP and UDP messages on ports 3005.
    let addr = "0.0.0.0:3005";
    network.listen_tcp(addr).unwrap();
    network.listen_udp(addr).unwrap();

    loop {
        match event_queue.receive() { // Read the next event or wait until have it.
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    InputMessage::HelloServer(msg) => {
                        network.send(endpoint, OutputMessage::HelloClient(msg)).unwrap();
                    },
                    //Other input messages here
                },
                NetEvent::AddedEndpoint(_endpoint, _address) => println!("TCP Client connected"),
                NetEvent::RemovedEndpoint(_endpoint) => println!("TCP Client disconnected"),
            },
            // Other events here
        }
    }
}
