use message_io::network::{Network, NetEvent, Transport};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum Message {
    Hello(String),
    // Other messages here
}

fn main() {
    let (mut network, mut events) = Network::split();

    // Listen for TCP, UDP and WebSocket messages.
    network.listen(Transport::Tcp, "0.0.0.0:3042").unwrap();
    network.listen(Transport::Udp, "0.0.0.0:3043").unwrap();
    network.listen(Transport::Ws, "0.0.0.0:3044").unwrap(); //WebSockets

    loop {
        match events.receive() { // Read the next event or wait until have it.
            NetEvent::Message(endpoint, message) => match message {
                Message::Hello(msg) => {
                    println!("Received: {}", msg);
                    network.send(endpoint, Message::Hello(msg));
                },
                //Other messages here
            },
            NetEvent::Connected(_endpoint) => println!("Client connected"), // Tcp or Ws
            NetEvent::Disconnected(_endpoint) => println!("Client disconnected"), //Tcp or Ws
            NetEvent::DeserializationError(_) => (),
        }
    }
}
