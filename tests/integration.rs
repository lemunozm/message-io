use message_io::network::{Network, NetEvent, Transport, SendStatus};
use message_io::{MAX_UDP_PAYLOAD_LEN};

use test_case::test_case;

use rand::{SeedableRng, Rng};

use std::collections::{HashSet};
use std::thread::{self, JoinHandle};
use std::net::{SocketAddr};
use std::time::{Duration};

pub const LOCAL_ADDR: &'static str = "127.0.0.1:0";
pub const SMALL_MESSAGE: &'static str = "Small message";
pub const BIG_MESSAGE_SIZE: usize = 1 << 20; // 1MB

// 8 is the Vec head offset added in serialization.
pub const MAX_SIZE_BY_UDP: usize = MAX_UDP_PAYLOAD_LEN - 8;
pub const MAX_SIZE_BY_WS: usize = MAX_UDP_PAYLOAD_LEN - 8;

lazy_static::lazy_static! {
    pub static ref TIMEOUT: Duration = Duration::from_millis(5000);
}

// Common error messages
pub const TIMEOUT_MSG_EXPECTED_ERR: &'static str = "Timeout, but a message was expected.";

mod util {
    use message_io::network::{Transport};

    use std::sync::{Once};

    // Used to init the log only one time for all tests;
    static INIT: Once = Once::new();

    #[allow(dead_code)]
    pub fn init_logger() {
        INIT.call_once(|| configure_logger().unwrap());
    }

    fn configure_logger() -> Result<(), fern::InitError> {
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{}][{}][{}][{}] {}",
                    chrono::Local::now().format("%H:%M:%S"),
                    record.level(),
                    record.target(),
                    std::thread::current().name().unwrap(),
                    message
                ))
            })
            .chain(std::io::stdout())
            .apply()?;
        Ok(())
    }

    pub const fn is_connection_oriented(transport: Transport) -> bool {
        match transport {
            Transport::Udp => false,
            Transport::Tcp => true,
            Transport::Ws => true,
        }
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
                            assert_eq!(SMALL_MESSAGE, String::from_utf8_lossy(&data));

                            let status = network.send(endpoint, &data);
                            assert_eq!(SendStatus::Sent, status);

                            messages_received += 1;

                            if !util::is_connection_oriented(transport) {
                                // We assume here that if the protocol is not connection oriented
                                // it will no create a resource.
                                // The remote will be managed from the listener resource
                                assert_eq!(listener_id, endpoint.resource_id());
                                if messages_received == expected_clients {
                                    break //Exit from thread.
                                }
                            }
                        }
                        NetEvent::Connected(endpoint) => {
                            match util::is_connection_oriented(transport) {
                                true => assert!(clients.insert(endpoint)),
                                false => unreachable!(),
                            }
                        }
                        NetEvent::Disconnected(endpoint) => {
                            match util::is_connection_oriented(transport) {
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
                    let status = network.send(server_endpoint, SMALL_MESSAGE.as_bytes());
                    assert_eq!(SendStatus::Sent, status);
                    assert!(clients.insert(server_endpoint));
                }

                loop {
                    match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                        NetEvent::Message(endpoint, data) => {
                            assert!(clients.remove(&endpoint));
                            assert_eq!(SMALL_MESSAGE, String::from_utf8_lossy(&data));
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

#[test_case(Transport::Udp, 1)]
#[test_case(Transport::Udp, 100)]
#[test_case(Transport::Tcp, 1)]
#[test_case(Transport::Tcp, 100)]
#[test_case(Transport::Ws, 1)]
#[test_case(Transport::Ws, 100)]
// NOTE: A medium-high `clients` value can exceeds the "open file" limits of an OS in CI
// with a very obfuscated error message.
fn echo(transport: Transport, clients: usize) {
    //util::init_logger();

    let (server_handle, server_addr) = echo_server_handle(transport, clients);
    let client_handle = echo_client_manager_handle(transport, server_addr, clients);

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

#[test_case(Transport::Tcp, BIG_MESSAGE_SIZE)]
#[test_case(Transport::Udp, MAX_SIZE_BY_UDP)]
#[test_case(Transport::Ws, MAX_SIZE_BY_WS)]
fn message_size(transport: Transport, message_size: usize) {
    //util::init_logger();

    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let sent_message: Vec<u8> = (0..message_size).map(|_| rng.gen()).collect();

    let server_handle = thread::Builder::new()
        .name("test-server".into())
        .spawn(move || {
            std::panic::catch_unwind(|| {
                let (mut network, mut event_queue) = Network::split();
                let (_, receiver_addr) = network.listen(transport, LOCAL_ADDR).unwrap();

                let (receiver, _) = network.connect(transport, receiver_addr).unwrap();
                let status = network.send(receiver, &sent_message);
                assert_eq!(status, SendStatus::Sent);

                loop {
                    match event_queue.receive_timeout(*TIMEOUT).expect(TIMEOUT_MSG_EXPECTED_ERR) {
                        NetEvent::Message(_, message) => break assert_eq!(sent_message, message),
                        NetEvent::Connected(_) => (),
                        NetEvent::Disconnected(_) => (),
                    }
                }
            })
            .unwrap();
        })
        .unwrap();

    server_handle.join().unwrap();
}
