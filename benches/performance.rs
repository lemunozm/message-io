use message_io::network::{self, NetworkController, NetworkProcessor, NetEvent, Transport};

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};

use std::time::{Duration};
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
        let (controller, mut processor) = network::split();
        processor.run(|_| ());

        let receiver_addr = controller.listen(transport, "127.0.0.1:0").unwrap().1;
        let receiver = controller.connect(transport, receiver_addr).unwrap().0;

        std::thread::sleep(std::time::Duration::from_millis(100));
        processor.stop();
        processor.wait();

        b.iter(|| {
            controller.send(receiver, &[0xFF]);
            processor.receive(Some(*SMALL_TIMEOUT), |_| ()); // It is ok, no cached event here.
        });
    });
}

/*
fn throughput_by(c: &mut Criterion, transport: Transport) {
    let sizes = [1, 2, 4, 8, 16, 32, 64, 128]
        .iter()
        .map(|i| i * 1024)
        .filter(|&size| size < transport.max_message_size());

    for block_size in sizes {
        let mut group = c.benchmark_group(format!("throughput by {}", transport));
        group.throughput(Throughput::Bytes(block_size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(block_size), &block_size, |b, &size| {
            let (network, mut events) = Network::split();
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
                events.receive_timeout(*SMALL_TIMEOUT).unwrap();
            });

            thread_running.store(false, Ordering::Relaxed);
            handle.join().unwrap();
        });
    }
}
*/

/// Latency test considerations:
/// The latency is adding the time to send&receive from the event queue, and maybe is a time that
/// is out of scope of this tests. So, we could be adding an extra latency.
/// How to avoid this time adition inside of Criterion framework?
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

fn throughput(c: &mut Criterion) {
/*
    #[cfg(feature = "udp")]
    throughput_by(c, Transport::Udp);
    //TODO: Fix this test: How to read inside of criterion iter()? an stream protocol?
    //#[cfg(feature = "tcp")] throughput_by(c, Transport::Tcp);
    #[cfg(feature = "tcp")]
    throughput_by(c, Transport::FramedTcp);
    #[cfg(feature = "websocket")]
    throughput_by(c, Transport::Ws);
*/
}

criterion_group!(benches, latency, throughput);
criterion_main!(benches);
