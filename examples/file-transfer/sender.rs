use super::common::{SenderMsg, ReceiverMsg};

use message_io::network::{NetEvent, Transport};
use message_io::node::{self, NodeEvent};

use std::fs::{self, File};
use std::io::{Read};
use std::time::{Duration};

enum Signal {
    SendChunk,
    // Other signals here
}

const CHUNK_SIZE: usize = 65536;

pub fn run(file_path: String) {
    let (handler, listener) = node::split();

    let server_addr = "127.0.0.1:3005";
    let (server_id, _) = handler.network().connect(Transport::FramedTcp, server_addr).unwrap();

    let file_size = fs::metadata(&file_path).unwrap().len() as usize;
    let mut file = File::open(&file_path).unwrap();
    let file_name: String = file_path.rsplit('/').into_iter().next().unwrap_or(&file_path).into();

    let mut file_bytes_sent = 0;
    listener.for_each(move |event| match event {
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Connected(_, established) => {
                if established {
                    println!("Sender connected by TCP at {}", server_addr);
                    let request = SenderMsg::FileRequest(file_name.clone(), file_size);
                    let output_data = bincode::serialize(&request).unwrap();
                    handler.network().send(server_id, &output_data);
                }
                else {
                    println!("Can not connect to the receiver by TCP to {}", server_addr)
                }
            }
            NetEvent::Accepted(_, _) => unreachable!(),
            NetEvent::Message(_, input_data) => {
                let message: ReceiverMsg = bincode::deserialize(&input_data).unwrap();
                match message {
                    ReceiverMsg::CanReceive(can) => match can {
                        true => handler.signals().send(Signal::SendChunk), // Start sending
                        false => {
                            handler.stop();
                            println!("The receiver can not receive the file :(");
                        }
                    },
                }
            }
            NetEvent::Disconnected(_) => {
                handler.stop();
                println!("\nReceiver disconnected");
            }
        },
        NodeEvent::Signal(signal) => match signal {
            Signal::SendChunk => {
                let mut data = [0; CHUNK_SIZE];
                let bytes_read = file.read(&mut data).unwrap();
                if bytes_read > 0 {
                    let chunk = SenderMsg::Chunk(Vec::from(&data[0..bytes_read]));
                    let output_data = bincode::serialize(&chunk).unwrap();
                    handler.network().send(server_id, &output_data);
                    file_bytes_sent += bytes_read;

                    let percentage = ((file_bytes_sent as f32 / file_size as f32) * 100.0) as usize;
                    print!("\rSending '{}': {}%", file_name, percentage);

                    handler.signals().send_with_timer(Signal::SendChunk, Duration::from_micros(10));
                }
                else {
                    println!("\nFile sent!");
                    handler.stop();
                }
            }
        },
    });
}
