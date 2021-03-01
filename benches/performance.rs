use message_io::network::{Network, NetEvent, Transport};

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};

use std::time::{Duration, Instant};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self};

lazy_static::lazy_static! {
    pub static ref SMALL_TIMEOUT: Duration = Duration::from_millis(100);
}

// Common error messages
pub const TIMEOUT_MSG_EXPECTED_ERR: &'static str = "Timeout, but a message was expected.";

fn latency_by(c: &mut Criterion, transport: Transport) {
    let msg = format!("latency by {}", transport);
    c.bench_function(&msg, |b| {
        let (mut events, mut network) = Network::split();

        let receiver_addr = network.listen(transport, "127.0.0.1:0").unwrap().1;
        let receiver = network.connect(transport, receiver_addr).unwrap().0;

        // skip the connection event for oriented connection protocols.
        events.receive_timeout(Duration::from_millis(100));

        b.iter(|| {
            network.send(receiver, &[0xFF]);
            loop {
                match events.receive_timeout(*SMALL_TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                    NetEvent::Message(_, _message) => break, // message received here
                    _ => (),
                }
            }
        });
    });
}

fn throughput_by(c: &mut Criterion, transport: Transport) {
    let sizes = [1, 2, 4, 8, 16, 32, 64, 128]
        .iter()
        .map(|i| i * 1024)
        .filter(|&size| size < transport.max_message_size());

    for block_size in sizes {
        let mut group = c.benchmark_group(format!("throughput by {}", transport));
        group.throughput(Throughput::Bytes(block_size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
            let (mut events, mut network) = Network::split();
            let receiver_addr = network.listen(transport, "127.0.0.1:0").unwrap().1;
            let receiver = network.connect(transport, receiver_addr).unwrap().0;

            // skip the connection event for oriented connection protocols.
            events.receive_timeout(*SMALL_TIMEOUT);

            let thread_running = Arc::new(AtomicBool::new(true));
            let running = thread_running.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let handle = thread::Builder::new()
                .name("test-server".into())
                .spawn(move || {
                    let message = (0..size).map(|_| 0xFF).collect::<Vec<u8>>();
                    tx.send(()).unwrap(); // receiving thread ready
                    while running.load(Ordering::Relaxed) {
                        network.send(receiver, &message);
                    }
                })
                .unwrap();

            rx.recv().unwrap();

            b.iter(|| {
                let start = Instant::now();
                events.receive_timeout(*SMALL_TIMEOUT).unwrap();
                start.elapsed()
            });

            thread_running.store(false, Ordering::Relaxed);
            handle.join().unwrap();
        });
    }
}

fn latency(c: &mut Criterion) {
    latency_by(c, Transport::Udp);
    latency_by(c, Transport::Tcp);
    latency_by(c, Transport::FramedTcp);
    latency_by(c, Transport::Ws);
}

fn throughput(c: &mut Criterion) {
    throughput_by(c, Transport::Udp);
    //throughput_by(c, Transport::Tcp); //TODO: Fix this test: How to read inside of iter()?
    throughput_by(c, Transport::FramedTcp);
    throughput_by(c, Transport::Ws);
}

criterion_group!(benches, latency, throughput);
criterion_main!(benches);
