use serde::{Serialize, Deserialize};

use std::time::Duration;

#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    Greet(String),
    Bye,
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    Greet(String),
    NotifyDisconnection(Duration),
    Bye,
}

