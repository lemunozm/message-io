use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent, TransportProtocol, Endpoint};

use std::net::{SocketAddr};

enum Event {
    Network(NetEvent<Message>),
}

pub struct Participant {
    event_queue: EventQueue<Event>,
    network: NetworkManager,
    name: String,
    discovery_endpoint: Endpoint,
    public_addr: SocketAddr,
}

impl Participant {
    pub fn new(name: &str) -> Option<Participant> {
        let mut event_queue = EventQueue::new();

        let network_sender = event_queue.sender().clone();
        let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

        // A listener for any other participant that want to establish connection.
        let listen_addr = "127.0.0.1:0".parse().unwrap();
        if let Some((_, addr)) = network.listen(listen_addr, TransportProtocol::Udp) {
            // Connection to the discovery server.
            let discovery_addr = "127.0.0.1:5000".parse().unwrap();
            if let Some((endpoint, _)) = network.connect(discovery_addr, TransportProtocol::Tcp) {
                Some(Participant {
                    event_queue,
                    network,
                    name: name.to_string(),
                    discovery_endpoint: endpoint,
                    public_addr: addr, // addr has the port that listen_addr has not.
                })
            }
            else {
                println!("Can not connect to the discovery server at {}", discovery_addr);
                None
            }
        }
        else {
            println!("Can not listen on {}", listen_addr);
            None
        }

    }

    pub fn run(mut self) {
        // Register this participant into the discovery server
        self.network.send(self.discovery_endpoint, Message::RegisterParticipant(self.name.clone(), self.public_addr));

        // Waiting events
        loop {
            match self.event_queue.receive() {
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(_, message) => match message {
                        Message::ParticipantList(participants) => {
                            println!("Participant list received ({} participants)", participants.len());
                            for (name, addr) in participants {
                                self.discovered_participant(&name, addr, "I see you in the participant list");
                            }
                        },
                        Message::ParticipantNotificationAdded(name, addr) => {
                            println!("New participant '{}' in the network", name);
                            self.discovered_participant(&name, addr, "welcome to the network!");
                        },
                        Message::ParticipantNotificationRemoved(name) => {
                            println!("Removed participant '{}' from the network", name);
                        },
                        Message::Gretings(name, gretings) => {
                            println!("'{}' says: {}", name, gretings);
                        }
                        _ => unreachable!(),
                    },
                    NetEvent::AddedEndpoint(_, _) => (),
                    NetEvent::RemovedEndpoint(endpoint) => {
                        if endpoint == self.discovery_endpoint {
                            return println!("Discovery server disconnected, closing");
                        }
                    },
                }
            }
        }
    }

    fn discovered_participant(&mut self, name: &str, addr: SocketAddr, message: &str) {
        if let Some((endpoint, _)) = self.network.connect(addr, TransportProtocol::Udp) {
            let gretings = format!("Hi '{}', {}", name, message);
            self.network.send(endpoint, Message::Gretings(self.name.clone(), gretings));
        }
    }
}
