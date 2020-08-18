use super::common::{ClientMessage, ServerMessage};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent, Endpoint};

use std::time::{Duration};
use std::net::{SocketAddr};
use std::collections::{HashMap};

enum Event {
    Network(NetEvent<ClientMessage>),
    Close,
    NotifyDisconnection,
    ProcessClientGretings(Endpoint, String),
}

pub fn run(is_tcp: bool) {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let mut clients: HashMap<Endpoint, SocketAddr> = HashMap::new();

    let protocol = if is_tcp { "tcp" } else { "udp" };
    let mut listen = |addr| match protocol {
        "tcp" => network.listen_tcp(addr),
        "udp" => network.listen_udp(addr),
        _ => unreachable!(),
    };

    let listen_addr = "127.0.0.1:3000";
    if let Some(_) = listen(listen_addr) {
        println!("Server running by {} at {}", protocol, listen_addr);
        event_queue.sender().send_with_timer(Event::NotifyDisconnection, Duration::from_secs(15));

        loop {
            match event_queue.receive() {
                Event::NotifyDisconnection => {
                    let disconnection_time = Duration::from_secs(10);
                    println!("The server will be disconnected in {} secs", disconnection_time.as_secs());
                    network.send_all(clients.keys(), ServerMessage::NotifyDisconnection(disconnection_time)).ok();
                    event_queue.sender().send_with_timer(Event::Close, disconnection_time);
                },
                Event::Close => {
                    println!("Closing server");
                    network.send_all(clients.keys(), ServerMessage::Bye).ok();
                    for endpoint in clients.keys() {
                        network.remove_endpoint(*endpoint);
                    }
                    return;
                },
                Event::ProcessClientGretings(endpoint, text) => {
                    println!("Client {} says: {}", clients[&endpoint], text);
                    network.send(endpoint, ServerMessage::Greet(String::from("Hi! I hear you")));
                }
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(endpoint, message) => match message {
                        ClientMessage::Greet(text) => {
                            if protocol == "udp" && !clients.contains_key(&endpoint) {
                                // Since UDP is not connection-oriented, we manualy generate an AddedEndpoint network event to add the client.
                                let addr = network.endpoint_remote_address(endpoint).unwrap();
                                event_queue.sender().send(Event::Network(NetEvent::AddedEndpoint(endpoint, addr)));
                            }
                            // In case of UDP, the if the AddedEndpoint has been sent, it will be processed before this
                            event_queue.sender().send(Event::ProcessClientGretings(endpoint, text));
                        },
                        ClientMessage::Bye => {
                            println!("Client {} closed", clients[&endpoint]);
                            if protocol == "udp" {
                                // Since UDP is not connection oriented, we manualy generate a RemovedEndpoint network event to remove the client.
                                event_queue.sender().send(Event::Network(NetEvent::RemovedEndpoint(endpoint)))
                            }
                        }
                    },
                    NetEvent::AddedEndpoint(endpoint, addr) => {
                        clients.insert(endpoint, addr);
                        println!("Client {} connected (total clients: {})", addr, clients.len());
                    },
                    NetEvent::RemovedEndpoint(endpoint) => {
                        let addr = clients.remove(&endpoint).unwrap();
                        println!("Client {} disconnected (total clients: {})", addr, clients.len());
                    }
                },
            }
        }
    }
    else {
        println!("Can not listening at selected interface/port");
    }
}
