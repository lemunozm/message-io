use message_io::network::{NetEvent, Transport};
use message_io::node::{self};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let my_name = match args.get(1) {
        Some(name) => name,
        None => return println!("Please, choose a name"),
    };

    let (handler, listener) = node::split::<()>();

    let multicast_addr = "239.255.0.1:3010";
    let (endpoint, _) = handler.network().connect(Transport::Udp, multicast_addr).unwrap();

    listener.for_each(move |event| match event.network() {
        NetEvent::Connected(_, _always_true_for_udp) => {
            println!("Notifying on the network");
            handler.network().send(endpoint, my_name.as_bytes());

            // Since the address belongs to the multicast range (from 224.0.0.0 to 239.255.255.255)
            // the internal resource will be configured to receive multicast messages.
            handler.network().listen(Transport::Udp, multicast_addr).unwrap();
        }
        NetEvent::Accepted(_, _) => unreachable!(), // UDP is not connection-oriented
        NetEvent::Message(_, data) => {
            println!("{} greets to the network!", String::from_utf8_lossy(&data));
        }
        NetEvent::Disconnected(_) => (),
    });
}
