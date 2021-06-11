use super::common::{Message};

use message_io::network::{NetEvent, Transport, Endpoint};
use message_io::node::{self, NodeHandler, NodeListener};

use std::net::{SocketAddr};
use std::collections::{HashMap};
use std::io::{self};

struct ParticipantInfo {
    addr: SocketAddr,
    endpoint: Endpoint,
}

pub struct DiscoveryServer {
    handler: NodeHandler<()>,
    node_listener: Option<NodeListener<()>>,
    participants: HashMap<String, ParticipantInfo>,
}

impl DiscoveryServer {
    pub fn new() -> io::Result<DiscoveryServer> {
        let (handler, node_listener) = node::split::<()>();

        let listen_addr = "127.0.0.1:5000";
        handler.network().listen(Transport::FramedTcp, listen_addr)?;

        println!("Discovery server running at {}", listen_addr);

        Ok(DiscoveryServer {
            handler,
            node_listener: Some(node_listener),
            participants: HashMap::new(),
        })
    }

    pub fn run(mut self) {
        let node_listener = self.node_listener.take().unwrap();
        node_listener.for_each(move |event| match event.network() {
            NetEvent::Connected(_, _) => unreachable!(), // There is no connect() calls.
            NetEvent::Accepted(_, _) => (),              // All endpoint accepted
            NetEvent::Message(endpoint, input_data) => {
                let message: Message = bincode::deserialize(&input_data).unwrap();
                match message {
                    Message::RegisterParticipant(name, addr) => {
                        self.register(&name, addr, endpoint);
                    }
                    Message::UnregisterParticipant(name) => {
                        self.unregister(&name);
                    }
                    _ => unreachable!(),
                }
            }
            NetEvent::Disconnected(endpoint) => {
                // Participant disconnection without explict unregistration.
                // We must remove from the registry too.
                let participant =
                    self.participants.iter().find(|(_, info)| info.endpoint == endpoint);

                if let Some(participant) = participant {
                    let name = participant.0.to_string();
                    self.unregister(&name);
                }
            }
        });
    }

    fn register(&mut self, name: &str, addr: SocketAddr, endpoint: Endpoint) {
        if !self.participants.contains_key(name) {
            // Update the new participant with the whole participants information
            let list =
                self.participants.iter().map(|(name, info)| (name.clone(), info.addr)).collect();

            let message = Message::ParticipantList(list);
            let output_data = bincode::serialize(&message).unwrap();
            self.handler.network().send(endpoint, &output_data);

            // Notify other participants about this new participant
            let message = Message::ParticipantNotificationAdded(name.to_string(), addr);
            let output_data = bincode::serialize(&message).unwrap();
            for participant in &mut self.participants {
                self.handler.network().send(participant.1.endpoint, &output_data);
            }

            // Register participant
            self.participants.insert(name.to_string(), ParticipantInfo { addr, endpoint });
            println!("Added participant '{}' with ip {}", name, addr);
        }
        else {
            println!(
                "Participant with name '{}' already exists, please registry with another name",
                name
            );
        }
    }

    fn unregister(&mut self, name: &str) {
        if let Some(info) = self.participants.remove(name) {
            // Notify other participants about this removed participant
            let message = Message::ParticipantNotificationRemoved(name.to_string());
            let output_data = bincode::serialize(&message).unwrap();
            for participant in &mut self.participants {
                self.handler.network().send(participant.1.endpoint, &output_data);
            }
            println!("Removed participant '{}' with ip {}", name, info.addr);
        }
        else {
            println!("Can not unregister an non-existent participant with name '{}'", name);
        }
    }
}
