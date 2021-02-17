use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum FromClientMessage {
    Ping, // name
}

#[derive(Serialize, Deserialize)]
pub enum FromServerMessage {
    Pong(usize), // count. Used for connection oriented protocols
    UnknownPong, // Used for non-connection oriented protocols
}
