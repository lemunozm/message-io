use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum FromClientMessage {
    Ping(String), // name
}

#[derive(Serialize, Deserialize)]
pub enum FromServerMessage {
    Pong(usize), // count
}
