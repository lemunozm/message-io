use message_io::network::{NetEvent, Transport};
use message_io::node::{self};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let my_name = match args.get(1) {
        Some(name) => name,
        None => return println!("Please, choose a name"),
    };

    let (node, listener) = node::split::<()>();

    let addr = "239.255.0.1:3010";
    match node.network().connect(Transport::Udp, addr) {
        Ok((endpoint, _)) => {
            println!("Notifying on the network");
            node.network().send(endpoint, my_name.as_bytes());
        }
        Err(_) => return eprintln!("Could not connect to {}", addr),
    }

    // Since the addrs belongs to the multicast range (from 224.0.0.0 to 239.255.255.255)
    // the internal resource will be configured to receive multicast messages.
    node.network().listen(Transport::Udp, addr).unwrap();

    listener.for_each(move |event| match event.network() {
        NetEvent::Message(_, data) => {
            println!("{} greets to the network!", String::from_utf8_lossy(&data));
        }
        NetEvent::Connected(_, _) => (),
        NetEvent::Disconnected(_) => (),
    });
}
