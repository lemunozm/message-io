use super::common::{FromServerMessage, FromClientMessage};

use message_io::network::{NetEvent, Transport, RemoteAddr};
use message_io::node::{self, NodeEvent};

use std::time::{Duration};

enum Signal {
    // This is a self event called every second.
    Greet,
    // Other signals here,
}

pub fn run(transport: Transport, remote_addr: RemoteAddr) {
    let (handler, listener) = node::split();

    let server_id = match handler.network().connect(transport, remote_addr.clone()) {
        Ok((server_id, local_addr)) => {
            println!("Connected to server by {} at {}", transport, server_id.addr());
            println!("Client identified by local port: {}", local_addr.port());
            server_id
        }
        Err(_) => {
            return println!("Can not connect to the server by {} to {}", transport, remote_addr)
        }
    };

    handler.signals().send(Signal::Greet);

    listener.for_each(move |event| match event {
        NodeEvent::Signal(signal) => match signal {
            Signal::Greet => {
                let message = FromClientMessage::Ping;
                let output_data = bincode::serialize(&message).unwrap();
                handler.network().send(server_id, &output_data);
                handler.signals().send_with_timer(Signal::Greet, Duration::from_secs(1));
            }
        },
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Message(_, input_data) => {
                let message: FromServerMessage = bincode::deserialize(&input_data).unwrap();
                match message {
                    FromServerMessage::Pong(count) => {
                        println!("Pong from server: {} times", count)
                    }
                    FromServerMessage::UnknownPong => println!("Pong from server"),
                }
            }
            NetEvent::Connected(_, _) => unreachable!(), // Only generated when a listener accepts
            NetEvent::Disconnected(_) => {
                println!("Server is disconnected");
                handler.stop();
            }
        },
    });
}
