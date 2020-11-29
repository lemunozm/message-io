use message_io::network::{NetworkManager, NetEvent};

use criterion::{criterion_group, criterion_main, Criterion};

use serde::{Serialize, Deserialize};

#[macro_use]
extern crate serde_big_array;
big_array! { BigArray; }

const SMALL_SIZE: usize = 16;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct SmallMessage {
    data: [u8; SMALL_SIZE]
}
const SMALL_MESSAGE: SmallMessage = SmallMessage{data: [0xFF; SMALL_SIZE]};

const MEDIUM_SIZE: usize = 1024;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct MediumMessage {
    #[serde(with = "BigArray")]
    data: [u8; MEDIUM_SIZE]
}
const MEDIUM_MESSAGE: MediumMessage = MediumMessage{data: [0xFF; MEDIUM_SIZE]};

const BIG_SIZE: usize = 65536;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct BigMessage {
    #[serde(with = "BigArray")]
    data: [u8; BIG_SIZE]
}
const BIG_MESSAGE: BigMessage = BigMessage{data: [0xFF; BIG_SIZE]};

#[derive(Debug)]
enum Transport {
    Tcp,
    Udp,
}

fn send_message<M>(c: &mut Criterion, message: M, transport: Transport)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {

    // We need the internal network thread running while sending messages
    let mut recv_network = NetworkManager::new(|_: NetEvent<M>| ());

    let receiver_addr = match transport {
        Transport::Tcp => recv_network.listen_tcp("127.0.0.1:0").unwrap().1,
        Transport::Udp => recv_network.listen_udp("127.0.0.1:0").unwrap().1,
    };

    let mut send_network = NetworkManager::new(|_: NetEvent<M>| ());

    let receiver = match transport {
        Transport::Tcp => send_network.connect_tcp(receiver_addr).unwrap(),
        Transport::Udp => send_network.connect_udp(receiver_addr).unwrap(),
    };

    let msg = format!("Sending {} bytes by {:?}", std::mem::size_of::<M>(), transport);
    c.bench_function(&msg, |b| {
        b.iter(|| {
            // The following process encodes, serializes and sends:
            send_network.send(receiver, message).unwrap();
        });
    });
}

fn send_while_recv_message<M>(c: &mut Criterion, message: M, transport: Transport)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {

    // The sender will send to a listener in the same network.
    // This emulates the process of sending messages while
    // other instace are sending also to the first one.
    let mut network = NetworkManager::new(|_: NetEvent<M>| ());

    let receiver_addr = match transport {
        Transport::Tcp => network.listen_tcp("127.0.0.1:0").unwrap().1,
        Transport::Udp => network.listen_udp("127.0.0.1:0").unwrap().1,
    };

    let receiver = match transport {
        Transport::Tcp => network.connect_tcp(receiver_addr).unwrap(),
        Transport::Udp => network.connect_udp(receiver_addr).unwrap(),
    };

    let msg = format!(
        "Sending {} bytes by {:?} (while recv)",
        std::mem::size_of::<M>(),
        transport
    );

    c.bench_function(&msg, |b| {
        b.iter(|| {
            // The following process encodes, serializes and sends:
            network.send(receiver, message).unwrap();
        });
    });
}

fn send_message_size_transport(c: &mut Criterion) {
    send_message(c, SMALL_MESSAGE, Transport::Tcp);
    send_message(c, MEDIUM_MESSAGE, Transport::Tcp);
    send_message(c, BIG_MESSAGE, Transport::Tcp);
    send_message(c, SMALL_MESSAGE, Transport::Udp);
    send_message(c, MEDIUM_MESSAGE, Transport::Udp);
}

fn send_message_while_recv_size_transport(c: &mut Criterion) {
    send_while_recv_message(c, SMALL_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, MEDIUM_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, BIG_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, SMALL_MESSAGE, Transport::Udp);
    send_while_recv_message(c, MEDIUM_MESSAGE, Transport::Udp);
}

criterion_group!(
    benches,
    send_message_size_transport,
    send_message_while_recv_size_transport
);

criterion_main!(benches);