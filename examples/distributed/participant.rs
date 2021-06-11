use super::common::{Message};

use message_io::network::{NetEvent, Transport, Endpoint};
use message_io::node::{self, NodeHandler, NodeListener};

use std::net::{SocketAddr};
use std::collections::{HashMap};
use std::io::{self};

pub struct Participant {
    handler: NodeHandler<()>,
    node_listener: Option<NodeListener<()>>,
    name: String,
    discovery_endpoint: Endpoint,
    public_addr: SocketAddr,
    known_participants: HashMap<String, Endpoint>, // Used only for free resources later
    greetings: HashMap<Endpoint, (String, String)>,
}

impl Participant {
    pub fn new(name: &str) -> io::Result<Participant> {
        let (handler, node_listener) = node::split();

        // A node_listener for any other participant that want to establish connection.
        // Returned 'listen_addr' contains the port that the OS gives for us when we put a 0.
        let listen_addr = "127.0.0.1:0";
        let (_, listen_addr) = handler.network().listen(Transport::FramedTcp, listen_addr)?;

        let discovery_addr = "127.0.0.1:5000"; // Connection to the discovery server.
        let (endpoint, _) = handler.network().connect(Transport::FramedTcp, discovery_addr)?;

        Ok(Participant {
            handler,
            node_listener: Some(node_listener),
            name: name.to_string(),
            discovery_endpoint: endpoint,
            public_addr: listen_addr,
            known_participants: HashMap::new(),
            greetings: HashMap::new(),
        })
    }

    pub fn run(mut self) {
        // Register this participant into the discovery server
        let node_listener = self.node_listener.take().unwrap();
        node_listener.for_each(move |event| match event.network() {
            NetEvent::Connected(endpoint, established) => {
                if endpoint == self.discovery_endpoint {
                    if established {
                        let message =
                            Message::RegisterParticipant(self.name.clone(), self.public_addr);
                        let output_data = bincode::serialize(&message).unwrap();
                        self.handler.network().send(self.discovery_endpoint, &output_data);
                    }
                    else {
                        println!("Can not connect to the discovery server");
                    }
                }
                else {
                    // Participant endpoint
                    let (name, message) = self.greetings.remove(&endpoint).unwrap();
                    if established {
                        let greetings = format!("Hi '{}', {}", name, message);
                        let message = Message::Greetings(self.name.clone(), greetings);
                        let output_data = bincode::serialize(&message).unwrap();
                        self.handler.network().send(endpoint, &output_data);
                        self.known_participants.insert(name.clone(), endpoint);
                    }
                }
            }
            NetEvent::Accepted(_, _) => (),
            NetEvent::Message(_, input_data) => {
                let message: Message = bincode::deserialize(&input_data).unwrap();
                match message {
                    Message::ParticipantList(participants) => {
                        println!("Participant list received ({} participants)", participants.len());
                        for (name, addr) in participants {
                            let text = "I see you in the participant list";
                            self.discovered_participant(&name, addr, text);
                        }
                    }
                    Message::ParticipantNotificationAdded(name, addr) => {
                        println!("New participant '{}' in the network", name);
                        self.discovered_participant(&name, addr, "welcome to the network!");
                    }
                    Message::ParticipantNotificationRemoved(name) => {
                        println!("Removed participant '{}' from the network", name);
                        if let Some(endpoint) = self.known_participants.remove(&name) {
                            self.handler.network().remove(endpoint.resource_id());
                        }
                    }
                    Message::Greetings(name, greetings) => {
                        println!("'{}' says: {}", name, greetings);
                    }
                    _ => unreachable!(),
                }
            }
            NetEvent::Disconnected(endpoint) => {
                if endpoint == self.discovery_endpoint {
                    println!("Discovery server disconnected, closing");
                    self.handler.stop();
                }
            }
        });
    }

    fn discovered_participant(&mut self, name: &str, addr: SocketAddr, text: &str) {
        let (endpoint, _) = self.handler.network().connect(Transport::FramedTcp, addr).unwrap();
        // Save the necessary info to send the message when the connection is established.
        self.greetings.insert(endpoint, (name.into(), text.into()));
    }
}
