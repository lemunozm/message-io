use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use criterion::{criterion_group, criterion_main, Criterion};

use serde::{Serialize, Deserialize};

#[macro_use]
extern crate serde_big_array;
big_array! { BigArray; }

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::io::prelude::*;

const SMALL_SIZE: usize = 16;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct SmallMessage {
    data: [u8; SMALL_SIZE],
}
const SMALL_MESSAGE: SmallMessage = SmallMessage { data: [0xFF; SMALL_SIZE] };

const MEDIUM_SIZE: usize = 1024;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct MediumMessage {
    #[serde(with = "BigArray")]
    data: [u8; MEDIUM_SIZE],
}
const MEDIUM_MESSAGE: MediumMessage = MediumMessage { data: [0xFF; MEDIUM_SIZE] };

const BIG_SIZE: usize = 65536;
#[derive(Serialize, Deserialize, Clone, Copy)]
struct BigMessage {
    #[serde(with = "BigArray")]
    data: [u8; BIG_SIZE],
}
const BIG_MESSAGE: BigMessage = BigMessage { data: [0xFF; BIG_SIZE] };

#[derive(Debug)]
enum Transport {
    Tcp,
    Udp,
}

//======================================================================
//                     std benches (used as reference)
//======================================================================

/// Measure the cost of sending a message using std::net::UdpSocket
fn std_send_udp<M>(c: &mut Criterion, message: M)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!("std-ref: Sending {} bytes by Udp", std::mem::size_of::<M>());
    c.bench_function(&msg, |b| {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = receiver.local_addr().unwrap();

        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.connect(addr).unwrap();
        let mut buffer = Vec::with_capacity(std::mem::size_of::<M>());

        b.iter(|| {
            buffer.clear();
            bincode::serialize_into(&mut buffer, &message).unwrap();
            socket.send(&buffer).unwrap();
        });
    });
}

/// Measure the cost of sending a message using std::net::TcpStream
fn std_send_tcp<M>(c: &mut Criterion, message: M)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!("std-ref: Sending {} bytes by Tcp", std::mem::size_of::<M>());
    c.bench_function(&msg, |b| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // Created only for not saturate the stream
        let receiver_handle = std::thread::spawn(move || {
            let mut receiver_stream = listener.incoming().next().unwrap().unwrap();
            loop {
                if let Ok(0) = receiver_stream.read(&mut [0; BIG_SIZE]) {
                    break
                }
            }
        });

        let stream = TcpStream::connect(addr).unwrap();
        let mut buffer = Vec::with_capacity(std::mem::size_of::<M>());

        {
            let stream = stream;
            b.iter(|| {
                buffer.clear();
                bincode::serialize_into(&mut buffer, &message).unwrap();
                (&stream).write(&buffer).unwrap();
            });
            //stream is destroyed and closed here
        }

        receiver_handle.join().unwrap();
    });
}

/// Measure the cost of sending messages and processing it using std::net::TcpStream
fn std_send_recv_tcp_one_direction<M>(c: &mut Criterion, message: M)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!(
        "std-ref: Sending and receiving one direction. {} bytes by Tcp",
        std::mem::size_of::<M>()
    );
    c.bench_function(&msg, |b| {
        b.iter_custom(|iters| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let stream = TcpStream::connect(addr).unwrap();
            let mut buffer = Vec::with_capacity(std::mem::size_of::<M>());

            let time = std::time::Instant::now();
            let receiver_handle = std::thread::spawn(move || {
                let mut receiver_stream = listener.incoming().next().unwrap().unwrap();
                loop {
                    //TODO: Take into account the deserialization
                    if let Ok(0) = receiver_stream.read(&mut [0; BIG_SIZE]) {
                        break
                    }
                }
            });

            {
                let stream = stream;
                for _ in 0..iters {
                    buffer.clear();
                    bincode::serialize_into(&mut buffer, &message).unwrap();
                    (&stream).write(&buffer).unwrap();
                }
                //stream is destroyed and closed here
            }

            receiver_handle.join().unwrap();
            time.elapsed()
        });
    });
}

//======================================================================
//                         message-io benches
//======================================================================

/// Measure the cost of sending a message by message-io
fn send_message<M>(c: &mut Criterion, message: M, transport: Transport)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!("Sending {} bytes by {:?}", std::mem::size_of::<M>(), transport);
    c.bench_function(&msg, |b| {
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

        b.iter(|| {
            // The following process encodes, serializes and sends:
            send_network.send(receiver, message).unwrap();
        });
    });
}

/// Measure the cost of sending a message by message-io
/// while at same time the network is receiving messages.
fn send_while_recv_message<M>(c: &mut Criterion, message: M, transport: Transport)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!("Sending {} bytes by {:?} (while recv)", std::mem::size_of::<M>(), transport);
    c.bench_function(&msg, |b| {
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

        b.iter(|| {
            // The following process encodes, serializes and sends:
            network.send(receiver, message).unwrap();
        });
    });
}

/// Measure the cost of sending messages and processing it by message-io
fn send_recv_tcp_one_direction<M>(c: &mut Criterion, message: M)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg =
        format!("Sending and receiving one direction {} bytes by Tcp", std::mem::size_of::<M>());
    c.bench_function(&msg, |b| {
        b.iter_custom(|iters| {
            let mut recv_event_queue = EventQueue::<NetEvent<M>>::new();
            let sender = recv_event_queue.sender().clone();
            let mut recv_network = NetworkManager::new(move |net_event| sender.send(net_event));

            let (_, receiver_addr) = recv_network.listen_tcp("127.0.0.1:0").unwrap();

            let mut send_event_queue = EventQueue::<NetEvent<M>>::new();
            let sender = send_event_queue.sender().clone();
            let mut send_network = NetworkManager::new(move |net_event| sender.send(net_event));

            let receiver = send_network.connect_tcp(receiver_addr).unwrap();

            let time = std::time::Instant::now();
            let receiver_handle = std::thread::spawn(move || {
                // Pass the network to the thread.
                // The network should be destroyed before event queue.
                let _ = recv_network;
                let mut received = 0;
                loop {
                    if let NetEvent::Message(..) = recv_event_queue.receive() {
                        received += 1;
                        if received == iters {
                            break
                        }
                    }
                }
            });

            for _ in 0..iters {
                send_network.send(receiver, message).unwrap();
            }

            receiver_handle.join().unwrap();
            time.elapsed()
        });
    });
}

fn std_send_recv_tcp_size(c: &mut Criterion) {
    std_send_recv_tcp_one_direction(c, SMALL_MESSAGE);
    std_send_recv_tcp_one_direction(c, MEDIUM_MESSAGE);
}

fn send_recv_tcp_size(c: &mut Criterion) {
    send_recv_tcp_one_direction(c, SMALL_MESSAGE);
    send_recv_tcp_one_direction(c, MEDIUM_MESSAGE);
    // Generates stack overflow. The sender is faster than receiver, and the EventQueue get filled.
    //send_recv_tcp_one_direction(c, BIG_MESSAGE);
}

fn std_send_size_transport(c: &mut Criterion) {
    std_send_tcp(c, SMALL_MESSAGE);
    std_send_tcp(c, MEDIUM_MESSAGE);
    std_send_tcp(c, BIG_MESSAGE);
    std_send_udp(c, SMALL_MESSAGE);
    std_send_udp(c, MEDIUM_MESSAGE);
}

fn send_size_transport(c: &mut Criterion) {
    send_message(c, SMALL_MESSAGE, Transport::Tcp);
    send_message(c, MEDIUM_MESSAGE, Transport::Tcp);
    send_message(c, BIG_MESSAGE, Transport::Tcp);
    send_message(c, SMALL_MESSAGE, Transport::Udp);
    send_message(c, MEDIUM_MESSAGE, Transport::Udp);
}

fn send_while_recv_size_transport(c: &mut Criterion) {
    send_while_recv_message(c, SMALL_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, MEDIUM_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, BIG_MESSAGE, Transport::Tcp);
    send_while_recv_message(c, SMALL_MESSAGE, Transport::Udp);
    send_while_recv_message(c, MEDIUM_MESSAGE, Transport::Udp);
}

criterion_group!(
    benches,
    std_send_size_transport,
    std_send_recv_tcp_size,
    send_recv_tcp_size,
    send_size_transport,
    send_while_recv_size_transport
);

criterion_main!(benches);
