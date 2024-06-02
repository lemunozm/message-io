use message_io::network::{NetEvent, Transport};
use message_io::node::{self};
use message_io::adapters::{udp};
use message_io::util::encoding::{self, Decoder, MAX_ENCODED_SIZE};

use tungstenite::{Message, connect as ws_connect, accept as ws_accept};
use url::{Url};

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self},
};
use std::io::{self, Write, Read};

const EXPECTED_BYTES: usize = 1024 * 1024 * 1024; // 1GB

const CHUNK: usize = udp::MAX_LOCAL_PAYLOAD_LEN;

fn main() {
    println!("Sending 1GB in chunks of {} bytes:\n", CHUNK);
    throughput_message_io(Transport::Udp, CHUNK);
    throughput_message_io(Transport::Tcp, CHUNK);
    throughput_message_io(Transport::FramedTcp, CHUNK);
    throughput_message_io(Transport::Ws, CHUNK);
    // for platforms that support it
    #[cfg(feature = "unixsocket")]
    throughput_message_io(Transport::UnixSocketStream, CHUNK);
    #[cfg(feature = "unixsocket")]
    throughput_message_io(Transport::UnixSocketDatagram, CHUNK);
    println!();
    throughput_native_udp(CHUNK);
    throughput_native_tcp(CHUNK);
    throughput_native_framed_tcp(CHUNK);
    throughput_native_ws(CHUNK);
}

fn throughput_message_io(transport: Transport, packet_size: usize) {
    print!("message-io {}:  \t", transport);
    io::stdout().flush().unwrap();

    let (handler, listener) = node::split::<()>();
    let message = (0..packet_size).map(|_| 0xFF).collect::<Vec<u8>>();

    let (_, addr) = handler.network().listen(transport, "127.0.0.1:0").unwrap();
    let (endpoint, _) = handler.network().connect(transport, addr).unwrap();

    let (t_ready, r_ready) = mpsc::channel();
    let (t_time, r_time) = mpsc::channel();

    let mut task = {
        let mut received_bytes = 0;
        let handler = handler.clone();

        listener.for_each_async(move |event| match event.network() {
            NetEvent::Connected(_, _) => (),
            NetEvent::Accepted(_, _) => t_ready.send(()).unwrap(),
            NetEvent::Message(_, data) => {
                received_bytes += data.len();
                if received_bytes >= EXPECTED_BYTES {
                    handler.stop();
                    t_time.send(Instant::now()).unwrap();
                }
            }
            NetEvent::Disconnected(_) => (),
        })
    };

    if transport.is_connection_oriented() {
        r_ready.recv().unwrap();
    }

    // Ensure that the connection is performed,
    // the internal thread is initialized for not oriented connection protocols
    // and we are waiting in the internal poll for data.
    std::thread::sleep(Duration::from_millis(100));

    let start_time = Instant::now();
    while handler.is_running() {
        handler.network().send(endpoint, &message);
    }

    let end_time = r_time.recv().unwrap();
    let elapsed = end_time - start_time;
    println!("Throughput: {}", ThroughputMeasure(EXPECTED_BYTES, elapsed));

    task.wait();
}

fn throughput_native_udp(packet_size: usize) {
    print!("native Udp: \t\t");
    io::stdout().flush().unwrap();

    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = receiver.local_addr().unwrap();

    let message = (0..packet_size).map(|_| 0xFF).collect::<Vec<u8>>();
    let mut buffer: [u8; CHUNK] = [0; CHUNK];

    let running = Arc::new(AtomicBool::new(true));
    let thread = {
        let running = running.clone();
        let message = message.clone();
        std::thread::Builder::new()
            .name("sender".into())
            .spawn(move || {
                let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
                sender.connect(addr).unwrap();
                let start_time = Instant::now();
                while running.load(Ordering::Relaxed) {
                    sender.send(&message).unwrap();
                }
                start_time
            })
            .unwrap()
    };

    let mut total_received = 0;
    while total_received < EXPECTED_BYTES {
        total_received += receiver.recv(&mut buffer).unwrap();
    }
    let end_time = Instant::now();
    running.store(false, Ordering::Relaxed);

    let start_time = thread.join().unwrap();
    let elapsed = end_time - start_time;
    println!("Throughput: {}", ThroughputMeasure(EXPECTED_BYTES, elapsed));
}

fn throughput_native_tcp(packet_size: usize) {
    print!("native Tcp: \t\t");
    io::stdout().flush().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let message = (0..packet_size).map(|_| 0xFF).collect::<Vec<u8>>();
    let mut buffer: [u8; CHUNK] = [0; CHUNK];

    let thread = {
        let message = message.clone();
        std::thread::Builder::new()
            .name("sender".into())
            .spawn(move || {
                let mut total_sent = 0;
                let mut sender = TcpStream::connect(addr).unwrap();

                let start_time = Instant::now();
                while total_sent < EXPECTED_BYTES {
                    sender.write(&message).unwrap();
                    total_sent += message.len();
                }
                start_time
            })
            .unwrap()
    };

    let (mut receiver, _) = listener.accept().unwrap();
    let mut total_received = 0;
    while total_received < EXPECTED_BYTES {
        total_received += receiver.read(&mut buffer).unwrap();
    }
    let end_time = Instant::now();

    let start_time = thread.join().unwrap();
    let elapsed = end_time - start_time;
    println!("Throughput: {}", ThroughputMeasure(EXPECTED_BYTES, elapsed));
}

fn throughput_native_framed_tcp(packet_size: usize) {
    print!("native FramedTcp: \t");
    io::stdout().flush().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let message = (0..packet_size).map(|_| 0xFF).collect::<Vec<u8>>();
    let mut buffer: [u8; CHUNK] = [0; CHUNK];

    let thread = {
        let message = message.clone();
        std::thread::Builder::new()
            .name("sender".into())
            .spawn(move || {
                let mut total_sent = 0;
                let mut sender = TcpStream::connect(addr).unwrap();
                let mut framming = [0; MAX_ENCODED_SIZE]; // used to avoid a heap allocation

                let start_time = Instant::now();
                while total_sent < EXPECTED_BYTES {
                    let encoded_size = encoding::encode_size(&message, &mut framming);
                    sender.write(&encoded_size).unwrap();
                    sender.write(&message).unwrap();
                    total_sent += message.len();
                }
                start_time
            })
            .unwrap()
    };

    let (mut receiver, _) = listener.accept().unwrap();
    let mut total_received = 0;
    let mut decoder = Decoder::default();
    while total_received < EXPECTED_BYTES {
        let mut message_received = false;
        while !message_received {
            let bytes = receiver.read(&mut buffer).unwrap();
            decoder.decode(&buffer[0..bytes], |decoded_data| {
                total_received += decoded_data.len();
                message_received = true;
            });
        }
    }
    let end_time = Instant::now();

    let start_time = thread.join().unwrap();
    let elapsed = end_time - start_time;
    println!("Throughput: {}", ThroughputMeasure(EXPECTED_BYTES, elapsed));
}

fn throughput_native_ws(packet_size: usize) {
    print!("native Ws: \t\t");
    io::stdout().flush().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let message = (0..packet_size).map(|_| 0xFF).collect::<Vec<u8>>();

    let thread = {
        let mut total_sent = 0;
        let message = message.clone();
        std::thread::Builder::new()
            .name("sender".into())
            .spawn(move || {
                let url_addr = format!("ws://{}/socket", addr);
                let (mut sender, _) = ws_connect(Url::parse(&url_addr).unwrap()).unwrap();
                let start_time = Instant::now();
                while total_sent < EXPECTED_BYTES {
                    sender.write_message(Message::Binary(message.clone())).unwrap();
                    total_sent += message.len();
                }
                start_time
            })
            .unwrap()
    };

    let mut receiver = ws_accept(listener.accept().unwrap().0).unwrap();
    let mut total_received = 0;
    while total_received < EXPECTED_BYTES {
        total_received += receiver.read_message().unwrap().len();
    }
    let end_time = Instant::now();

    let start_time = thread.join().unwrap();
    let elapsed = end_time - start_time;
    println!("Throughput: {}", ThroughputMeasure(EXPECTED_BYTES, elapsed));
}

pub struct ThroughputMeasure(usize, Duration); //bytes, elapsed
impl std::fmt::Display for ThroughputMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes_per_sec = self.0 as f64 / self.1.as_secs_f64();
        if bytes_per_sec < 1000.0 {
            write!(f, "{:.2} B/s", bytes_per_sec)
        }
        else if bytes_per_sec < 1000_000.0 {
            write!(f, "{:.2} KB/s", bytes_per_sec / 1000.0)
        }
        else if bytes_per_sec < 1000_000_000.0 {
            write!(f, "{:.2} MB/s", bytes_per_sec / 1000_000.0)
        }
        else {
            write!(f, "{:.2} GB/s", bytes_per_sec / 1000_000_000.0)
        }
    }
}
