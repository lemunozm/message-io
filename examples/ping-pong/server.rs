use super::common::{FromServerMessage, FromClientMessage};

use message_io::network::{NetEvent, Transport, Endpoint};
use message_io::node::{self};

use std::collections::{HashMap};
use std::net::{SocketAddr};

struct ClientInfo {
    count: usize,
}

pub fn run(transport: Transport, addr: SocketAddr) {
    let (node, listener) = node::split::<()>();

    let mut clients: HashMap<Endpoint, ClientInfo> = HashMap::new();

    match node.network().listen(transport, addr) {
        Ok((_resource_id, real_addr)) => {
            println!("Server running at {} by {}", real_addr, transport)
        }
        Err(_) => return println!("Can not listening at {} by {}", addr, transport),
    }

    listener.for_each(move |event| match event.network() {
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
                    node.network().send(endpoint, &output_data);
                }
            }
        }
        NetEvent::Connected(endpoint, _) => {
            // Only connection oriented protocols will generate this event
            clients.insert(endpoint, ClientInfo { count: 0 });
            println!("Client ({}) connected (total clients: {})", endpoint.addr(), clients.len());
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
    });
}
