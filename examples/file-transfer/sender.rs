use super::common::{SenderMsg, ReceiverMsg};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use std::fs::{self, File};
use std::io::{Read};

enum Event {
    Network(NetEvent<ReceiverMsg>),
    SendChunk,
}

pub fn run(file_path: &str) {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network =
        NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let server_addr = "127.0.0.1:3005";
    let server_id = match network.connect_tcp(server_addr) {
        Ok(server_id) => {
            println!("Connect to receiver by TCP at {}", server_addr);
            server_id
        }
        Err(_) => return println!("Can not connect to the receiver by TCP to {}", server_addr),
    };

    let file_size = fs::metadata(&file_path).unwrap().len() as usize;
    let mut file = File::open(file_path).unwrap();
    let mut file_bytes_sent = 0;
    const CHUNK_SIZE: usize = 65536;

    let file_name = file_path.rsplit('/').into_iter().next().unwrap_or(file_path);
    let request = SenderMsg::FileRequest(file_name.into(), file_size);
    network.send(server_id, request).unwrap();

    loop {
        match event_queue.receive() {
            Event::Network(net_event) => match net_event {
                NetEvent::Message(_, message) => match message {
                    ReceiverMsg::CanReceive(can) => {
                        if can {
                            event_queue.sender().send(Event::SendChunk);
                        }
                        else {
                            return println!("The receiver can not receive the file :(")
                        }
                    }
                },
                NetEvent::AddedEndpoint(_) => unreachable!(),
                NetEvent::RemovedEndpoint(_) => return println!("\nReceiver disconnected"),
            },
            Event::SendChunk => {
                let mut data = [0; CHUNK_SIZE];
                let bytes_read = file.read(&mut data).unwrap();
                if bytes_read > 0 {
                    let chunk = SenderMsg::Chunk(Vec::from(&data[0..bytes_read]));
                    network.send(server_id, chunk).unwrap();
                    file_bytes_sent += bytes_read;
                    event_queue.sender().send(Event::SendChunk);

                    let percentage = ((file_bytes_sent as f32 / file_size as f32) * 100.0) as usize;
                    print!("\rSending '{}': {}%", file_name, percentage);
                }
                else {
                    return println!("\nFile sent!")
                }
            }
        }
    }
}
