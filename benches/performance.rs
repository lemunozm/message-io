use message_io::network::{self, Transport, NetworkController, NetworkProcessor, Endpoint};
use message_io::util::thread::{NamespacedThread};

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};

use std::time::{Duration};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

lazy_static::lazy_static! {
    static ref TIMEOUT: Duration = Duration::from_millis(100);
}

fn init_connection(transport: Transport) -> (NetworkController, NetworkProcessor, Endpoint) {
    let (controller, mut processor) = network::split();

    let running = Arc::new(AtomicBool::new(true));
    let mut thread = {
        let running = running.clone();
        NamespacedThread::spawn("perf-listening", move || {
            while running.load(Ordering::Relaxed) {
                processor.process_poll_event(Some(*TIMEOUT), |_| ());
            }
            processor
        })
    };

    let receiver_addr = controller.listen(transport, "127.0.0.1:0").unwrap().1;
    let receiver = controller.connect(transport, receiver_addr).unwrap().0;

    running.store(false, Ordering::Relaxed);
    let processor = thread.join();
    // From here, the connection is performed independently of the transport used

    (controller, processor, receiver)
}

fn latency_by(c: &mut Criterion, transport: Transport) {
    let msg = format!("latency by {}", transport);
    c.bench_function(&msg, |b| {
        let (controller, mut processor, endpoint) = init_connection(transport);

        b.iter(|| {
            controller.send(endpoint, &[0xFF]);
            processor.process_poll_event(Some(*TIMEOUT), |_| ());
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
            let (controller, mut processor, endpoint) = init_connection(transport);

            let thread_running = Arc::new(AtomicBool::new(true));
            let running = thread_running.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let mut thread = NamespacedThread::spawn("perf-sender", move || {
                let message = (0..size).map(|_| 0xFF).collect::<Vec<u8>>();
                tx.send(()).unwrap(); // receiving thread ready
                while running.load(Ordering::Relaxed) {
                    controller.send(endpoint, &message);
                }
            });
            rx.recv().unwrap();

            b.iter(|| {
                // FIX IT:
                // Because the sender do not stop sends, the receiver has always data.
                // This means that only one poll event is generated for all messages, and
                // process_poll_event will call the callback continuously without ends.
                processor.process_poll_event(Some(*TIMEOUT), |_| ());
            });

            thread_running.store(true, Ordering::Relaxed);
            thread.join();
        });
    }
}

fn latency(c: &mut Criterion) {
    #[cfg(feature = "udp")]
    latency_by(c, Transport::Udp);
    #[cfg(feature = "tcp")]
    latency_by(c, Transport::Tcp);
    #[cfg(feature = "tcp")]
    latency_by(c, Transport::FramedTcp);
    #[cfg(feature = "websocket")]
    latency_by(c, Transport::Ws);
}

#[allow(dead_code)] //TODO: remove when the throughput test works fine
fn throughput(c: &mut Criterion) {
    #[cfg(feature = "udp")]
    throughput_by(c, Transport::Udp);
    // TODO: Fix this test: How to read inside of criterion iter() an stream protocol?
    // #[cfg(feature = "tcp")]
    // throughput_by(c, Transport::Tcp);
    #[cfg(feature = "tcp")]
    throughput_by(c, Transport::FramedTcp);
    #[cfg(feature = "websocket")]
    throughput_by(c, Transport::Ws);
}

criterion_group!(benches, latency /*throughput*/,);
criterion_main!(benches);
