// IMPORTANT!!
// This file is not currently maintained.
// It will be updated in https://github.com/lemunozm/message-io/pull/34
use message_io::network::{Network, NetEvent, Transport, SendStatus};
use message_io::encoding::{self, Decoder};
use message_io::MAX_UDP_PAYLOAD_LEN;

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput, black_box};

#[macro_use]
extern crate serde_big_array;
big_array! { BigArray; }

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::io::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use std::thread::{self};
use std::io::{ErrorKind};

lazy_static::lazy_static! {
    pub static ref TIMEOUT: Duration = Duration::from_millis(5000);
    pub static ref SMALL_TIMEOUT: Duration = Duration::from_millis(100);
}

// Common error messages
pub const TIMEOUT_MSG_EXPECTED_ERR: &'static str = "Timeout, but a message was expected.";

fn base_latency_by_tcp(c: &mut Criterion) {
    let msg = format!("[base] latency by tcp");
    c.bench_function(&msg, |b| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut sender_stream = TcpStream::connect(addr).unwrap();
        let mut receiver_stream = listener.incoming().next().unwrap().unwrap();

        let mut input_buffer: [u8; encoding::PADDING + 1] = [0; encoding::PADDING + 1];
        let message = [0xFF];
        b.iter(|| {
            sender_stream.write(&message).unwrap();
            receiver_stream.read(&mut input_buffer).unwrap();
        });
    });
}

fn base_latency_by_udp(c: &mut Criterion) {
    let msg = format!("[base] latency by udp");
    c.bench_function(&msg, |b| {
        let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.connect(addr).unwrap();

        let mut input_buffer: [u8; 1] = [0; 1];
        let message = [0xFF];
        b.iter(|| {
            socket.send(&message).unwrap();
            listener.recv(&mut input_buffer).unwrap();
        });
    });
}

fn latency_by(c: &mut Criterion, transport: Transport) {
    let msg = format!("latency by {}", transport);
    c.bench_function(&msg, |b| {
        let (mut network, mut events) = Network::split();

        let receiver_addr = network.listen(transport, "127.0.0.1:0").unwrap().1;
        let receiver = network.connect(transport, receiver_addr).unwrap().0;

        //skipped the connection event for oriented connection protocols.
        events.receive_timeout(Duration::from_millis(100));

        b.iter(|| {
            network.send(receiver, &[0xFF]);
            loop {
                match events.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                    NetEvent::Message(_, _message) => break, // message received here
                    _ => (),
                }
            }
        });
    });
}

/*
fn base_throughput_by_tcp(c: &mut Criterion) {
    let block_size = std::u16::MAX as usize;

    let mut group = c.benchmark_group("base throughput by tcp");
    group.throughput(Throughput::Bytes(block_size as u64));
    group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut sender = TcpStream::connect(addr).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::Builder::new()
            .name("test-server".into())
            .spawn(move || {
                let mut input_buffer = (0..block_size).map(|_| 0).collect::<Vec<u8>>();
                let mut receiver = listener.incoming().next().unwrap().unwrap();
                receiver.set_read_timeout(Some(*SMALL_TIMEOUT)).unwrap();

                tx.send(()).unwrap(); // receiving thread ready
                loop {
                    match receiver.read(&mut input_buffer) {
                        Ok(_) => (),
                        Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(err) => Err(err).unwrap(),
                    }
                }
            })
            .unwrap();

        rx.recv().unwrap();
        // Receiving thread ready, we can start to send now.

        let message = (0..size - 8).map(|_| 0xFF).collect::<Vec<u8>>();
        b.iter(|| {
            sender.write(&message).unwrap();
        });

        handle.join().unwrap();
    });
    group.finish();
}

fn base_throughput_by_udp(c: &mut Criterion) {
    let block_size = MAX_UDP_PAYLOAD_LEN; //MAX UDP PAYLOAD LINUX

    let mut group = c.benchmark_group("base throughput by udp");
    group.throughput(Throughput::Bytes(block_size as u64));
    group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
        let reader = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = reader.local_addr().unwrap();

        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(addr).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::Builder::new()
            .name("test-server".into())
            .spawn(move || {
                let mut input_buffer = (0..block_size).map(|_| 0).collect::<Vec<u8>>();
                reader.set_read_timeout(Some(*SMALL_TIMEOUT)).unwrap();

                tx.send(()).unwrap(); // receiving thread ready
                loop {
                    match reader.recv(&mut input_buffer) {
                        Ok(_) => (),
                        Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(err) => Err(err).unwrap(),
                    }
                }
            })
            .unwrap();

        rx.recv().unwrap();
        // Receiving thread ready, we can start to send now.

        let message = (0..size - 8).map(|_| 0xFF).collect::<Vec<u8>>();
        let mut output_buffer = Vec::with_capacity(size);
        b.iter(|| {
            output_buffer.clear();
            bincode::serialize_into(&mut output_buffer, &message).unwrap();
            sender.send(&output_buffer).unwrap();
        });

        handle.join().unwrap();
    });
    group.finish();
}

fn throughput_by(c: &mut Criterion, transport: Transport) {
    let block_size = match transport {
        Transport::Udp => MAX_UDP_PAYLOAD_LEN,
        Transport::Tcp => std::u16::MAX as usize * 10,
        _ => 0, //TODO,
    };

    let mut group = c.benchmark_group(format!("throughput by {}", transport));
    group.throughput(Throughput::Bytes(block_size as u64));
    group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
        let (mut network, mut events) = Network::split::<Vec<u8>>();

        let receiver_addr = network.listen(transport, "127.0.0.1:0").unwrap().1;
        let receiver = network.connect(transport, receiver_addr).unwrap().0;

        // skipped the connection event for oriented connection protocols.
        events.receive_timeout(Duration::from_millis(100));

        // serializing Vec adds 8 bytes
        let message = (0..size - 8).map(|_| 0xFF).collect::<Vec<u8>>();
        b.iter(|| {
            let status = network.send(receiver, message.clone());
            assert_eq!(status, SendStatus::Sent);
        });

        loop {
            match events.receive_timeout(Duration::from_millis(100)) {
                Some(_) => (),
                None => break, // No more messages to read
            }
        }
    });
}

fn throughput_by(c: &mut Criterion, transport: Transport) {
    let block_size = match transport {
        Transport::Udp => MAX_UDP_PAYLOAD_LEN,
        Transport::Tcp => std::u16::MAX as usize,
        Transport::Ws => std::u16::MAX as usize,
    };

    let mut group = c.benchmark_group(format!("throughput by {}", transport));
    group.throughput(Throughput::Bytes(block_size as u64));
    group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
        // serializing Vec adds 8 bytes
        b.iter_custom(|iters| {
            let (mut network, mut events) = Network::split::<Vec<u8>>();

            let receiver_addr = network.listen(transport, "127.0.0.1:0").unwrap().1;
            let receiver = network.connect(transport, receiver_addr).unwrap().0;

            // Skip possible connection event.
            events.receive_timeout(*SMALL_TIMEOUT);

            let thread_running = Arc::new(AtomicBool::new(true));
            let running = thread_running.clone();

            let (tx, rx) = std::sync::mpsc::channel();
            let handle = thread::Builder::new()
                .name("test-server".into())
                .spawn(move || {
                    let message = (0..size - 8).map(|_| 0xFF).collect::<Vec<u8>>();
                    tx.send(()).unwrap(); // receiving thread ready
                    while running.load(Ordering::Relaxed) {
                        let status = network.send(receiver, message.clone());
                        assert_eq!(status, SendStatus::Sent);
                    }
                })
                .unwrap();

            rx.recv().unwrap();

            let start = Instant::now();
            for _ in 0..iters {
                events.receive_timeout(*SMALL_TIMEOUT).unwrap();
            }
            let interval = start.elapsed();

            thread_running.store(false, Ordering::Relaxed);
            handle.join().unwrap();
            interval
        });
    });
}
*/

fn latency(c: &mut Criterion) {
    //base_latency_by_tcp(c);
    latency_by(c, Transport::Tcp);
    //base_latency_by_udp(c);
    latency_by(c, Transport::Udp);
    latency_by(c, Transport::Ws);
}

/*
fn throughput(c: &mut Criterion) {
    //base_throughput_by_tcp(c);
    //throughput_by(c, Transport::Tcp);
    //base_throughput_by_udp(c);
    throughput_by(c, Transport::Tcp);
    throughput_by(c, Transport::Udp);
    throughput_by(c, Transport::Ws);
}
*/

criterion_group!(
    benches,
    latency,
    //throughput,
    /*
    std_send_size_transport,
    std_send_recv_tcp_size,
    send_recv_tcp_size,
    send_size_transport,
    send_while_recv_size_transport
    */
);

criterion_main!(benches);
