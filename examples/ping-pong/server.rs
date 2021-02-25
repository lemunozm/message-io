use super::common::{FromServerMessage, FromClientMessage};

use message_io::network::{Network, NetEvent, Endpoint, Transport};

use std::collections::{HashMap};
use std::net::{SocketAddr};

struct ClientInfo {
    count: usize,
}

pub fn run(transport: Transport, addr: SocketAddr) {
    let (mut event_queue, mut network) = Network::split();

    let mut clients: HashMap<Endpoint, ClientInfo> = HashMap::new();

    match network.listen(transport, addr) {
        Ok((_resource_id, real_addr)) => {
            println!("Server running at {} by {}", real_addr, transport)
        }
        Err(_) => return println!("Can not listening at {} by {}", addr, transport),
    }

    loop {
        match event_queue.receive() {
            // Also you can use receive_timeout
            NetEvent::Message(endpoint, input_data) => {
                let message: FromClientMessage = bincode::deserialize(&input_data).unwrap();
                match message {
                    FromClientMessage::Ping => {
                        let message = match clients.get_mut(&endpoint) {
                            Some(client) => {
                                // For connection oriented protocols
                                client.count += 1;
                                println!("Ping from {}, {} times", endpoint.addr(), client.count);
                                FromServerMessage::Pong(client.count)
                            }
                            None => {
                                // For non-connection oriented protocols
                                println!("Ping from {}", endpoint.addr());
                                FromServerMessage::UnknownPong
                            }
                        };
                        let output_data = bincode::serialize(&message).unwrap();
                        network.send(endpoint, &output_data);
                    }
                }
            }
            NetEvent::Connected(endpoint) => {
                // Only connection oriented protocols will generate this event
                clients.insert(endpoint, ClientInfo { count: 0 });
                println!(
                    "Client ({}) connected (total clients: {})",
                    endpoint.addr(),
                    clients.len()
                );
            }
            NetEvent::Disconnected(endpoint) => {
                // Only connection oriented protocols will generate this event
                clients.remove(&endpoint).unwrap();
                println!(
                    "Client ({}) disconnected (total clients: {})",
                    endpoint.addr(),
                    clients.len()
                );
            }
        }
    }
}
