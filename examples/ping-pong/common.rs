use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum FromClientMessage {
    Ping,
}

#[derive(Serialize, Deserialize)]
pub enum FromServerMessage {
    Pong(usize), // Used for connection oriented protocols
    UnknownPong, // Used for non-connection oriented protocols
}
