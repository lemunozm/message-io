use super::common::{FromServerMessage, FromClientMessage};

use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Transport, RemoteAddr};

use std::time::{Duration};

enum Event {
    Network(NetEvent<FromServerMessage>),

    // This is a self event called every second.
    // You can mix network events with your own events in the EventQueue.
    Greet,
}

pub fn run(transport: Transport, remote_addr: RemoteAddr) {
    let mut event_queue = EventQueue::new();

    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(Event::Network(net_event)));

    let (server_id, local_addr) = match network.connect(Transport::Tcp, remote_addr.clone()) {
        Ok(conn_info) => conn_info,
        Err(_) => {
            return println!("Can not connect to the server by {:?} to {}", transport, remote_addr)
        }
    };

    println!("Connect to server by TCP at {}", server_id.addr());
    println!("Client identified by local port: {}", local_addr.port());
    event_queue.sender().send(Event::Greet);

    loop {
        match event_queue.receive() {
            Event::Greet => {
                network.send(server_id, FromClientMessage::Ping);
                event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(1));
            }
            Event::Network(net_event) => match net_event {
                NetEvent::Message(_, message) => match message {
                    FromServerMessage::Pong(count) => {
                        println!("Pong from server: {} time", count)
                    }
                },
                NetEvent::Connected(_) => unreachable!(), // Only generated when listen
                NetEvent::Disconnected(_) => return println!("Server is disconnected"),
                NetEvent::DeserializationError(_) => (), // Malformed message from the server
            },
        }
    }
}
