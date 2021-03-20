use super::common::{FromServerMessage, FromClientMessage};

use message_io::network::{Network, NetEvent, Transport, RemoteAddr};

use std::time::{Duration};

enum Event {
    Network(NetEvent),

    // This is a self event called every second.
    // You can mix network events with your own events in the EventQueue.
    // For that, call Network::split_and_map() to map to your custom event.
    Greet,
}

pub fn run(transport: Transport, remote_addr: RemoteAddr) {
    let (network, mut events) = Network::split_and_map(|net_event| Event::Network(net_event));

    let (server_id, local_addr) = match network.connect(transport, remote_addr.clone()) {
        Ok(conn_info) => conn_info,
        Err(_) => {
            return println!("Can not connect to the server by {} to {}", transport, remote_addr)
        }
    };

    println!("Connected to server by {} at {}", transport, server_id.addr());
    println!("Client identified by local port: {}", local_addr.port());
    events.sender().send(Event::Greet);

    loop {
        match events.receive() {
            Event::Greet => {
                let message = FromClientMessage::Ping;
                let output_data = bincode::serialize(&message).unwrap();
                network.send(server_id, &output_data);
                events.sender().send_with_timer(Event::Greet, Duration::from_secs(1));
            }
            Event::Network(net_event) => match net_event {
                NetEvent::Message(_, input_data) => {
                    let message: FromServerMessage = bincode::deserialize(&input_data).unwrap();
                    match message {
                        FromServerMessage::Pong(count) => {
                            println!("Pong from server: {} times", count)
                        }
                        FromServerMessage::UnknownPong => println!("Pong from server"),
                    }
                }
                NetEvent::Connected(_, _) => unreachable!(), // Only generated when listen
                NetEvent::Disconnected(_) => return println!("Server is disconnected"),
            },
        }
    }
}
