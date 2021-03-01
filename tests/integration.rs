use message_io::network::{Network, NetEvent, Transport, SendStatus};

use test_case::test_case;

use rand::{SeedableRng, Rng};

use std::collections::{HashSet};
use std::thread::{self, JoinHandle};
use std::net::{SocketAddr};
use std::time::{Duration};

const LOCAL_ADDR: &'static str = "127.0.0.1:0";
const MIN_MESSAGE: &'static [u8] = &[42];
const SMALL_MESSAGE: &'static str = "Integration test message";
const BIG_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB

lazy_static::lazy_static! {
    pub static ref TIMEOUT: Duration = Duration::from_millis(5000);
}

// Common error messages
const TIMEOUT_MSG_EXPECTED_ERR: &'static str = "Timeout, but a message was expected.";

mod util {
    use std::sync::{Once};

    // Used to init the log only one time for all tests;
    static INIT: Once = Once::new();

    #[allow(dead_code)]
    pub fn init_logger() {
        INIT.call_once(|| configure_logger().unwrap());
    }

    fn configure_logger() -> Result<(), fern::InitError> {
        fern::Dispatch::new()
            .filter(|metadata| metadata.target().starts_with("message_io"))
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}][{}][{}] {}",
                    chrono::Local::now().format("%M:%S:%f"), // min:sec:nano
                    record.level(),
                    record.target(),
                    std::thread::current().name().unwrap(),
                    message,
                ))
            })
            .chain(std::io::stdout())
            .apply()?;
        Ok(())
    }
}

fn echo_server_handle(
    transport: Transport,
    expected_clients: usize,
) -> (JoinHandle<()>, SocketAddr)
{
    let (tx, rx) = crossbeam::channel::bounded(1);
    let handle = thread::Builder::new()
        .name("test-server".into())
        .spawn(move || {
            std::panic::catch_unwind(|| {
                let (mut network, mut event_queue) = Network::split();

                let (listener_id, server_addr) = network.listen(transport, LOCAL_ADDR).unwrap();
                tx.send(server_addr).unwrap();

                let mut messages_received = 0;
                let mut disconnections = 0;
                let mut clients = HashSet::new();

                loop {
                    match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                        NetEvent::Message(endpoint, data) => {
                            assert_eq!(MIN_MESSAGE, data);

                            let status = network.send(endpoint, &data);
                            assert_eq!(SendStatus::Sent, status);

                            messages_received += 1;

                            if !transport.is_connection_oriented() {
                                // We assume here that if the protocol is not connection oriented
                                // it will no create a resource.
                                // The remote will be managed from the listener resource
                                assert_eq!(listener_id, endpoint.resource_id());
                                if messages_received == expected_clients {
                                    break //Exit from thread.
                                }
                            }
                        }
                        NetEvent::Connected(endpoint) => match transport.is_connection_oriented() {
                            true => assert!(clients.insert(endpoint)),
                            false => unreachable!(),
                        },
                        NetEvent::Disconnected(endpoint) => {
                            match transport.is_connection_oriented() {
                                true => {
                                    disconnections += 1;
                                    assert!(clients.remove(&endpoint));
                                    if disconnections == expected_clients {
                                        assert_eq!(expected_clients, messages_received);
                                        assert_eq!(0, clients.len());
                                        break //Exit from thread.
                                    }
                                }
                                false => unreachable!(),
                            }
                        }
                    }
                }
            })
            .unwrap();
        })
        .unwrap();

    let server_addr = rx.recv_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR);
    (handle, server_addr)
}

fn echo_client_manager_handle(
    transport: Transport,
    server_addr: SocketAddr,
    clients_number: usize,
) -> JoinHandle<()>
{
    thread::Builder::new()
        .name("test-client".into())
        .spawn(move || {
            std::panic::catch_unwind(|| {
                let (mut network, mut event_queue) = Network::split();

                let mut clients = HashSet::new();

                for _ in 0..clients_number {
                    let (server_endpoint, _) = network.connect(transport, server_addr).unwrap();
                    let status = network.send(server_endpoint, MIN_MESSAGE);
                    assert_eq!(SendStatus::Sent, status);
                    assert!(clients.insert(server_endpoint));
                }

                loop {
                    match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                        NetEvent::Message(endpoint, data) => {
                            assert!(clients.remove(&endpoint));
                            assert_eq!(MIN_MESSAGE, &data);
                            network.remove(endpoint.resource_id());
                            if clients.len() == 0 {
                                break //Exit from thread.
                            }
                        }
                        NetEvent::Connected(_) => unreachable!(),
                        NetEvent::Disconnected(_) => unreachable!(),
                    }
                }
            })
            .unwrap();
        })
        .unwrap()
}

fn burst_receiver_handle(
    transport: Transport,
    expected_count: usize,
) -> (JoinHandle<()>, SocketAddr)
{
    let (tx, rx) = crossbeam::channel::bounded(1);
    let handle = thread::Builder::new()
        .name("test-client".into())
        .spawn(move || {
            std::panic::catch_unwind(|| {
                let (mut network, mut event_queue) = Network::split();
                let (_, receiver_addr) = network.listen(transport, LOCAL_ADDR).unwrap();

                tx.send(receiver_addr).unwrap();

                let mut count = 0;
                loop {
                    match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                        NetEvent::Message(_, data) => {
                            let expected_message = format!("{}: {}", SMALL_MESSAGE, count);
                            assert_eq!(expected_message, String::from_utf8_lossy(&data));
                            count += 1;
                            if count == expected_count {
                                break
                            }
                        }
                        NetEvent::Connected(_) => (),
                        NetEvent::Disconnected(_) => (),
                    }
                }
            })
            .unwrap();
        })
        .unwrap();
    (handle, rx.recv().unwrap())
}

fn burst_sender_handle(
    transport: Transport,
    receiver_addr: SocketAddr,
    expected_count: usize,
) -> JoinHandle<()>
{
    thread::Builder::new()
        .name("test-client".into())
        .spawn(move || {
            std::panic::catch_unwind(|| {
                let mut network = Network::new(|_| ());

                let (receiver, _) = network.connect(transport, receiver_addr).unwrap();

                for count in 0..expected_count {
                    let message = format!("{}: {}", SMALL_MESSAGE, count);
                    let status = network.send(receiver, message.as_bytes());
                    assert_eq!(SendStatus::Sent, status);
                }
            })
            .unwrap();
        })
        .unwrap()
}

//#[test_case(Transport::Udp, 200000)] // Inestable: UDP can lost packets and mess them up.
//#[test_case(Transport::Tcp, 200000)] // Does not apply: Tcp is stream based
#[test_case(Transport::FramedTcp, 200000)]
#[test_case(Transport::Ws, 200000)]
fn burst(transport: Transport, messages_count: usize) {
    //util::init_logger(); // Enable it for better debugging

    let (receiver_handle, server_addr) = burst_receiver_handle(transport, messages_count);
    let sender_handle = burst_sender_handle(transport, server_addr, messages_count);

    receiver_handle.join().unwrap();
    sender_handle.join().unwrap();
}

#[test_case(Transport::Tcp, 1)]
#[test_case(Transport::Tcp, 100)]
#[test_case(Transport::Udp, 1)]
#[test_case(Transport::Udp, 100)]
#[test_case(Transport::FramedTcp, 1)]
#[test_case(Transport::FramedTcp, 100)]
#[test_case(Transport::Ws, 1)]
#[test_case(Transport::Ws, 100)]
// NOTE: A medium-high `clients` value can exceeds the "open file" limits of an OS in CI
// with an obfuscated error message.
fn echo(transport: Transport, clients: usize) {
    //util::init_logger(); // Enable it for better debugging

    let (server_handle, server_addr) = echo_server_handle(transport, clients);
    let client_handle = echo_client_manager_handle(transport, server_addr, clients);

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

#[test_case(Transport::Tcp, BIG_MESSAGE_SIZE)]
#[test_case(Transport::FramedTcp, BIG_MESSAGE_SIZE)]
#[test_case(Transport::Udp, Transport::Udp.max_message_size())]
#[test_case(Transport::Ws, BIG_MESSAGE_SIZE)]
fn message_size(transport: Transport, message_size: usize) {
    //util::init_logger(); // Enable it for better debugging

    assert!(!transport.is_packet_based() || message_size <= transport.max_message_size());

    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let sent_message: Vec<u8> = (0..message_size).map(|_| rng.gen()).collect();

    let (mut network, mut event_queue) = Network::split();
    let (_, receiver_addr) = network.listen(transport, LOCAL_ADDR).unwrap();

    let (receiver, _) = network.connect(transport, receiver_addr).unwrap();

    let sent_message_cloned = sent_message.clone();
    if !transport.is_connection_oriented() {
        let status = network.send(receiver, &sent_message_cloned);
        assert_eq!(status, SendStatus::Sent);
    }
    else {
        match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
            NetEvent::Connected(_) => {
                thread::Builder::new()
                    .name("test-client".into())
                    .spawn(move || {
                        let status = network.send(receiver, &sent_message_cloned);
                        assert_eq!(status, SendStatus::Sent);
                        network.remove(receiver.resource_id());
                    })
                    .unwrap();
            }
            _ => unreachable!(),
        }
    }

    let mut received_message = Vec::new();
    loop {
        match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
            NetEvent::Message(_, data) => {
                if transport.is_packet_based() {
                    received_message = data;
                    assert_eq!(sent_message, received_message);
                    break
                }
                else {
                    received_message.extend_from_slice(&data);
                }
            }
            NetEvent::Connected(_) => unreachable!(),
            NetEvent::Disconnected(_) => {
                assert_eq!(sent_message.len(), received_message.len());
                assert_eq!(sent_message, received_message);
                break
            }
        }
    }
}
