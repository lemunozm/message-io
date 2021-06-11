use super::common::{SenderMsg, ReceiverMsg};

use message_io::network::{NetEvent, Transport, Endpoint};
use message_io::node::{self};

use std::collections::{HashMap};
use std::fs::{File};
use std::io::{Write};

pub struct Transfer {
    file: File,
    name: String,
    current_size: usize,
    expected_size: usize,
}

pub fn run() {
    let (handler, listener) = node::split::<()>();

    let listen_addr = "127.0.0.1:3005";
    handler.network().listen(Transport::FramedTcp, listen_addr).unwrap();
    println!("Receiver running by TCP at {}", listen_addr);

    let mut transfers: HashMap<Endpoint, Transfer> = HashMap::new();

    listener.for_each(move |event| match event.network() {
        NetEvent::Connected(_, _) => unreachable!(),
        NetEvent::Accepted(_, _) => (),
        NetEvent::Message(endpoint, input_data) => {
            let message: SenderMsg = bincode::deserialize(&input_data).unwrap();
            match message {
                SenderMsg::FileRequest(name, size) => {
                    let able = match File::create(format!("{}.recv", name)) {
                        Ok(file) => {
                            println!("Accept file: '{}' with {} bytes", name, size);
                            let transfer =
                                Transfer { file, name, current_size: 0, expected_size: size };
                            transfers.insert(endpoint, transfer);
                            true
                        }
                        Err(_) => {
                            println!("Can not open the file to write");
                            false
                        }
                    };

                    let output_data = bincode::serialize(&ReceiverMsg::CanReceive(able)).unwrap();
                    handler.network().send(endpoint, &output_data);
                }
                SenderMsg::Chunk(data) => {
                    let transfer = transfers.get_mut(&endpoint).unwrap();
                    transfer.file.write(&data).unwrap();
                    transfer.current_size += data.len();

                    let current = transfer.current_size as f32;
                    let expected = transfer.expected_size as f32;
                    let percentage = ((current / expected) * 100.0) as usize;
                    print!("\rReceiving '{}': {}%", transfer.name, percentage);

                    if transfer.expected_size == transfer.current_size {
                        println!("\nFile '{}' received!", transfer.name);
                        transfers.remove(&endpoint).unwrap();
                    }
                }
            }
        }
        NetEvent::Disconnected(endpoint) => {
            // Unexpected sender disconnection. Cleaning.
            if transfers.contains_key(&endpoint) {
                println!("\nUnexpected Sender disconnected");
                transfers.remove(&endpoint);
            }
        }
    });
}
