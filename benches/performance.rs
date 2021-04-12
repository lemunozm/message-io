use message_io::network::{self, Transport, NetworkController, NetworkProcessor, Endpoint};
use message_io::util::thread::{NamespacedThread};

use criterion::{criterion_group, criterion_main, Criterion};

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

criterion_group!(benches, latency);
criterion_main!(benches);
