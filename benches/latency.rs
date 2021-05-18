use message_io::network::{self, Transport, NetworkController, NetworkProcessor, Endpoint};
use message_io::util::thread::{NamespacedThread};
use message_io::util::encoding::{self, Decoder, MAX_ENCODED_SIZE};

use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(feature = "websocket")]
use tungstenite::{Message, connect as ws_connect, accept as ws_accept};
#[cfg(feature = "websocket")]
use url::{Url};

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::{Duration};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::io::{Write, Read};

lazy_static::lazy_static! {
    static ref TIMEOUT: Duration = Duration::from_millis(1);
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
    let receiver = controller.connect_sync(transport, receiver_addr).unwrap().0;

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

fn latency_by_native_udp(c: &mut Criterion) {
    let msg = format!("latency by native Udp");
    c.bench_function(&msg, |b| {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = receiver.local_addr().unwrap();

        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(addr).unwrap();

        let mut buffer: [u8; 1] = [0; 1];

        b.iter(|| {
            sender.send(&[0xFF]).unwrap();
            receiver.recv(&mut buffer).unwrap();
        });
    });
}

fn latency_by_native_tcp(c: &mut Criterion) {
    let msg = format!("latency by native Tcp");
    c.bench_function(&msg, |b| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut sender = TcpStream::connect(addr).unwrap();
        let (mut receiver, _) = listener.accept().unwrap();

        let mut buffer: [u8; 1] = [0; 1];

        b.iter(|| {
            sender.write(&[0xFF]).unwrap();
            receiver.read(&mut buffer).unwrap();
        });
    });
}

fn latency_by_native_framed_tcp(c: &mut Criterion) {
    let msg = format!("latency by native FramedTcp");
    c.bench_function(&msg, |b| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut sender = TcpStream::connect(addr).unwrap();
        let (mut receiver, _) = listener.accept().unwrap();

        let mut buffer: [u8; 1] = [0; 1];
        let mut framming = [0; MAX_ENCODED_SIZE]; // used to avoid a heap allocation
        let mut decoder = Decoder::default();

        b.iter(|| {
            let encoded_size = encoding::encode_size(&[0xFF], &mut framming);
            sender.write(&encoded_size).unwrap();
            sender.write(&[0xFF]).unwrap();

            let mut message_received = false;
            while !message_received {
                let bytes = receiver.read(&mut buffer).unwrap();
                decoder.decode(&buffer[0..bytes], |_decoded_data| message_received = true);
            }
        });
    });
}

#[cfg(feature = "websocket")]
fn latency_by_native_web_socket(c: &mut Criterion) {
    let msg = format!("latency by native Ws");
    c.bench_function(&msg, |b| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let mut listen_thread = NamespacedThread::spawn("perf-listening", move || {
            ws_accept(listener.accept().unwrap().0).unwrap()
        });

        let url_addr = format!("ws://{}/socket", addr);
        let (mut sender, _) = ws_connect(Url::parse(&url_addr).unwrap()).unwrap();

        let mut receiver = listen_thread.join();

        let message = vec![0xFF];

        b.iter(|| {
            sender.write_message(Message::Binary(message.clone())).unwrap();
            receiver.read_message().unwrap();
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

    #[cfg(feature = "udp")]
    latency_by_native_udp(c);
    #[cfg(feature = "tcp")]
    latency_by_native_tcp(c);
    #[cfg(feature = "tcp")]
    latency_by_native_framed_tcp(c);
    #[cfg(feature = "websocket")]
    latency_by_native_web_socket(c);
}

criterion_group!(benches, latency);
criterion_main!(benches);
