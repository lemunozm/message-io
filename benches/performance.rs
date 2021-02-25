// IMPORTANT!!
// This file is not currently maintained.
// It will be updated in https://github.com/lemunozm/message-io/pull/34

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

//######################################################################
//                     std benches (used as reference)
//######################################################################

/// Measure the cost of sending a message using std::net::UdpSocket
fn std_send_udp<M>(c: &mut Criterion, message: M)
where M: Serialize + for<'b> Deserialize<'b> + Send + Copy + 'static {
    let msg = format!("STD-REF: Sending {} bytes by Udp", std::mem::size_of::<M>());
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
    let msg = format!("STD-REF: Sending {} bytes by Tcp", std::mem::size_of::<M>());
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
        "STD-REF: Sending and receiving one direction. {} bytes by Tcp",
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

//######################################################################
//                         message-io benches
//######################################################################

fn std_send_recv_tcp_size(c: &mut Criterion) {
    std_send_recv_tcp_one_direction(c, SMALL_MESSAGE);
    std_send_recv_tcp_one_direction(c, MEDIUM_MESSAGE);
}

fn std_send_size_transport(c: &mut Criterion) {
    std_send_tcp(c, SMALL_MESSAGE);
    std_send_tcp(c, MEDIUM_MESSAGE);
    std_send_tcp(c, BIG_MESSAGE);
    std_send_udp(c, SMALL_MESSAGE);
    std_send_udp(c, MEDIUM_MESSAGE);
}

criterion_group!(benches, std_send_size_transport, std_send_recv_tcp_size,);

criterion_main!(benches);
