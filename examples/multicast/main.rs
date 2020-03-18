use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent, TransportProtocol};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum Message {
    HelloLan(String),
}

enum Event {
    Network(NetEvent<Message>),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let my_name = match args.get(1) {
        Some(name) => name,
        None => return println!("Please choose a name")
    };

    let mut event_queue = EventQueue::new();

    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(Event::Network(net_event)));

    network.connect("239.255.0.1:3010".parse().unwrap(), TransportProtocol::Udp)
        //If the things goes well:
        .map(|(endpoint, _)| {
            network.send(endpoint, Message::HelloLan(my_name.into())).unwrap();
        }
    ).unwrap();
    network.listen("239.255.0.1:3010".parse().unwrap(), TransportProtocol::UdpMulticast).unwrap();

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(_, message) => match message {
                    Message::HelloLan(name) => {
                        println!("{} greets to the network!", name)
                    },
                },
                NetEvent::AddedEndpoint(_, _) => (),
                NetEvent::RemovedEndpoint(_) => (),
            },
            // Other events here
        }
    }
}
