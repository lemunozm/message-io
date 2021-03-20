use message_io::network::{Network, NetEvent, Transport};

enum Event {
    Net(NetEvent),
    Tick,
    // Any other app event here.
}

fn main() {
    // The split_and_map() version allows to combine network events with your application events.
    let (network, mut events) = Network::split_and_map(|net_event| Event::Net(net_event));

    // You can change the transport to Udp or Ws (WebSocket).
    let (server, _) = network.connect(Transport::FramedTcp, "127.0.0.1:3042").unwrap();

    events.sender().send(Event::Tick); // Start sending
    loop {
        match events.receive() {
            Event::Net(net_event) => match net_event { // event from the network
                NetEvent::Message(_endpoint, data) => {
                    println!("Received: {}", String::from_utf8_lossy(&data));
                }
                _ => (),
            },
            Event::Tick => { // computed every second
                network.send(server, "Hello server!".as_bytes());
                events.sender().send_with_timer(Event::Tick, std::time::Duration::from_secs(1));
            }
        }
    }
}
