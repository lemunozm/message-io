use super::common::{Message};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Endpoint, Transport};

use std::net::{SocketAddr};
use std::collections::{HashMap};

struct ParticipantInfo {
    addr: SocketAddr,
    endpoint: Endpoint,
}

pub struct DiscoveryServer {
    event_queue: EventQueue<NetEvent>,
    network: Network,
    participants: HashMap<String, ParticipantInfo>,
}

impl DiscoveryServer {
    pub fn new() -> Option<DiscoveryServer> {
        let (mut network, event_queue) = Network::split();

        let listen_addr = "127.0.0.1:5000";
        match network.listen(Transport::FramedTcp, listen_addr) {
            Ok(_) => {
                println!("Discovery server running at {}", listen_addr);
                Some(DiscoveryServer { event_queue, network, participants: HashMap::new() })
            }
            Err(_) => {
                println!("Can not listen on {}", listen_addr);
                None
            }
        }
    }

    pub fn run(mut self) {
        loop {
            match self.event_queue.receive() {
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
                NetEvent::Connected(_, _) => (),
                NetEvent::Disconnected(endpoint) => {
                    // Participant disconection without explict unregistration.
                    // We must remove from the registry too.
                    let participant_name = self.participants.iter().find_map(|(name, info)| {
                        match info.endpoint == endpoint {
                            true => Some(name.clone()),
                            false => None,
                        }
                    });

                    if let Some(name) = participant_name {
                        self.unregister(&name)
                    }
                }
            }
        }
    }

    fn register(&mut self, name: &str, addr: SocketAddr, endpoint: Endpoint) {
        if !self.participants.contains_key(name) {
            // Update the new participant with the whole participants information
            let list =
                self.participants.iter().map(|(name, info)| (name.clone(), info.addr)).collect();

            let message = Message::ParticipantList(list);
            let output_data = bincode::serialize(&message).unwrap();
            self.network.send(endpoint, &output_data);

            // Notify other participants about this new participant
            let message = Message::ParticipantNotificationAdded(name.to_string(), addr);
            let output_data = bincode::serialize(&message).unwrap();
            for participant in &mut self.participants {
                self.network.send(participant.1.endpoint, &output_data);
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
                self.network.send(participant.1.endpoint, &output_data);
            }
            println!("Removed participant '{}' with ip {}", name, info.addr);
        }
        else {
            println!("Can not unregister an non-existent participant with name '{}'", name);
        }
    }
}
