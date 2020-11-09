use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum SenderMsg {
    //From sender to receiver
    FileRequest(String, usize), // name, size
    Chunk(Vec<u8>),             // data
}

#[derive(Serialize, Deserialize)]
pub enum ReceiverMsg {
    //From receiver to sender
    CanReceive(bool),
}

/*
     ┌──────┐                                               ┌────────┐
     │Sender│                                               │Receiver│
     └──┬───┘                                               └───┬────┘
        │                      FileRequest                      │
        │ ──────────────────────────────────────────────────────>
        │                                                       │
        │ CanReceive (if the server can receive the file or not)│
        │ <──────────────────────────────────────────────────────
        │                                                       │
        │                         Chunk                         │
        │ ──────────────────────────────────────────────────────>
        │                                                       │
        .                                                       .
        .                                                       .
        .                                                       .
        .                                                       .
        │                         Chunk                         │
        │ ──────────────────────────────────────────────────────>
        │                                                       │
        │                         Chunk                         │
        │ ──────────────────────────────────────────────────────>
        │                                                       │
        │                         Chunk                         │
        │ ──────────────────────────────────────────────────────>
     ┌──┴───┐                                               ┌───┴────┐
     │Sender│                                               │Receiver│
     └──────┘                                               └────────┘
*/
