use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum FromClientMessage {
    Greetings(String),
}

#[derive(Serialize, Deserialize)]
pub enum FromServerMessage {
    CountGreetings(String, usize),
}
