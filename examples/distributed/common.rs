use serde::{Serialize, Deserialize};

use std::net::{SocketAddr};

#[derive(Serialize, Deserialize)]

pub enum Message {
    // To DiscoveryServer
    RegisterParticipant(String, SocketAddr),
    UnregisterParticipant(String),

    // From DiscoveryServer
    ParticipantList(Vec<(String, SocketAddr)>),
    ParticipantNotificationAdded(String, SocketAddr),
    ParticipantNotificationRemoved(String),

    // From Participant to Participant
    Greetings(String, String), //name and greetings
}
