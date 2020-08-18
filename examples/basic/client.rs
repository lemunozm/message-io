use super::common::{ClientMessage, ServerMessage};

use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use std::time::{Duration};

enum Event {
    Network(NetEvent<ServerMessage>),
    Greet,
}

pub fn run(is_tcp: bool) {
    let mut event_queue = EventQueue::new();

    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let protocol = if is_tcp { "tcp" } else { "udp" };
    let mut connect = |addr| match protocol {
        "tcp" => network.connect_tcp(addr),
        "udp" => network.connect_udp(addr),
        _ => unreachable!(),
    };

    let addr = "127.0.0.1:3000";
    if let Some((server, _)) = connect(addr) {
        println!("Talking to server by {} at {}", protocol, addr);
        event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(1));

        let mut hello_counter = 0;
        loop {
            match event_queue.receive() {
                Event::Greet => {
                    println!("Saying hello to the server... ({})", hello_counter);
                    network.send(server, ClientMessage::Greet(format!("Hello ({})", hello_counter)));
                    event_queue.sender().send_with_timer(Event::Greet, Duration::from_secs(2));
                    hello_counter += 1;
                }
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(_, message) => match message {
                        ServerMessage::Greet(text) => println!("Server says: {}", text),
                        ServerMessage::NotifyDisconnection(duration) => println!("Server notified disconnection in {} secs", duration.as_secs()),
                        ServerMessage::Bye => {
                            println!("Server say: good bye!");
                            if protocol == "udp" {
                                // Since UDP is not connection-oriented, we manualy generate a RemovedEndpoint network event to emulate a server disconnection and exit.
                                event_queue.sender().send(Event::Network(NetEvent::RemovedEndpoint(server)));
                            }
                        },
                    },
                    NetEvent::AddedEndpoint(_, _) => unreachable!(),
                    NetEvent::RemovedEndpoint(_) => {
                        println!("Server is disconnected");
                        return;
                    }
                }
            }
        }
    }
    else {
        println!("Can not connect to the server by {} to {}", protocol, addr);
    }
}
