use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum Message {
    Greetings(String),
}
