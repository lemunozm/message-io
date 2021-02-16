use super::common::{FromServerMessage, FromClientMessage};

use message_io::network::{Network, NetEvent, Endpoint, Transport};

use std::collections::{HashMap};
use std::net::{SocketAddr};

struct ClientInfo {
    count: usize,
}

pub fn run(transport: Transport, addr: SocketAddr) {
    let (mut network, mut event_queue) = Network::split();

    let mut clients: HashMap<Endpoint, ClientInfo> = HashMap::new();

    match network.listen(transport, addr) {
        Ok((_resource_id, real_addr)) => println!("TCP Server running at {}", real_addr),
        Err(_) => return println!("Can not listening at {}", addr),
    }

    loop {
        match event_queue.receive() {
            // Also you can use receive_timeout
            NetEvent::Message(endpoint, message) => match message {
                FromClientMessage::Ping => {
                    let count = &mut clients.get_mut(&endpoint).unwrap().count;
                    *count += 1;
                    println!("Ping from {}, {} time", endpoint.addr(), count);
                    network.send(endpoint, FromServerMessage::Pong(*count));
                }
            },
            NetEvent::Connected(endpoint) => {
                // Only connection oriented protocols as Tcp or Websocket will generate this event
                clients.insert(endpoint, ClientInfo { count: 0 });
                println!(
                    "Client ({}) connected (total clients: {})",
                    endpoint.addr(),
                    clients.len()
                );
            }
            NetEvent::Disconnected(endpoint) => {
                // Only connection oriented protocols as Tcp or Websocket will generate this event
                clients.remove(&endpoint).unwrap();
                println!(
                    "Client ({}) disconnected (total clients: {})",
                    endpoint.addr(),
                    clients.len()
                );
            }
            NetEvent::DeserializationError(_) => (), // Only if the user send a malformed message.
        }
    }
}
