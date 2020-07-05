use serde::{Serialize, Deserialize};

use std::time::Duration;

#[derive(Serialize, Deserialize)]
pub enum Message {
    Info(String),
    NotifyDisconnection(Duration),
    Bye,
}

