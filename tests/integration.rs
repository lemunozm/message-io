use message_io::network::{NetEvent, Transport, SendStatus};
use message_io::node::{self, NodeEvent};
use message_io::util::thread::{NamespacedThread};
use message_io::adapters::udp::{self};

use test_case::test_case;

use rand::{SeedableRng, Rng};

use std::collections::{HashSet};
use std::net::{SocketAddr};
use std::time::{Duration};

const LOCAL_ADDR: &'static str = "127.0.0.1:0";
const MIN_MESSAGE: &'static [u8] = &[42];
const SMALL_MESSAGE: &'static str = "Integration test message";
const BIG_MESSAGE_SIZE: usize = 1024 * 1024 * 8; // 8MB

lazy_static::lazy_static! {
    pub static ref TIMEOUT: Duration = Duration::from_secs(60);
    pub static ref TIMEOUT_SMALL: Duration = Duration::from_secs(1);
}

// Common error messages
const TIMEOUT_EVENT_RECV_ERR: &'static str = "Timeout, but an event was expected.";

mod util {
    use std::sync::{Once};

    // Used to init the log only one time for all tests;
    static INIT: Once = Once::new();

    #[allow(dead_code)]
    pub enum LogThread {
        Enabled,
        Disabled,
    }

    #[allow(dead_code)]
    pub fn init_logger(log_thread: LogThread) {
        INIT.call_once(|| configure_logger(log_thread).unwrap());
    }

    fn configure_logger(log_thread: LogThread) -> Result<(), fern::InitError> {
        fern::Dispatch::new()
            .filter(|metadata| metadata.target().starts_with("message_io"))
            .format(move |out, message, record| {
                let thread_name = format!("[{}]", std::thread::current().name().unwrap());
                out.finish(format_args!(
                    "[{}][{}][{}]{} {}",
                    chrono::Local::now().format("%M:%S:%f"), // min:sec:nano
                    record.level(),
                    record.target().strip_prefix("message_io::").unwrap_or(record.target()),
                    if let LogThread::Enabled = log_thread { thread_name } else { String::new() },
                    message,
                ))
            })
            .chain(std::io::stdout())
            .apply()?;
        Ok(())
    }
}

#[allow(unused_imports)]
use util::{LogThread};

fn start_echo_server(
    transport: Transport,
    expected_clients: usize,
) -> (NamespacedThread<()>, SocketAddr) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let thread = NamespacedThread::spawn("test-server", move || {
        let mut messages_received = 0;
        let mut disconnections = 0;
        let mut clients = HashSet::new();

        let (node, listener) = node::split();
        node.signals().send_with_timer((), *TIMEOUT);

        let (listener_id, server_addr) = node.network().listen(transport, LOCAL_ADDR).unwrap();
        tx.send(server_addr).unwrap();

        listener.for_each(move |event| match event {
            NodeEvent::Signal(_) => panic!("{}", TIMEOUT_EVENT_RECV_ERR),
            NodeEvent::Network(net_event) => match net_event {
                NetEvent::Connected(..) => unreachable!(),
                NetEvent::Accepted(endpoint, id) => {
                    assert_eq!(listener_id, id);
                    match transport.is_connection_oriented() {
                        true => assert!(clients.insert(endpoint)),
                        false => unreachable!(),
                    }
                }
                NetEvent::Message(endpoint, data) => {
                    assert_eq!(MIN_MESSAGE, data);

                    let status = node.network().send(endpoint, &data);
                    assert_eq!(SendStatus::Sent, status);

                    messages_received += 1;

                    if !transport.is_connection_oriented() {
                        // We assume here that if the protocol is not
                        // connection-oriented it will no create a resource.
                        // The remote will be managed from the listener resource
                        assert_eq!(listener_id, endpoint.resource_id());
                        if messages_received == expected_clients {
                            node.stop() //Exit from thread.
                        }
                    }
                }
                NetEvent::Disconnected(endpoint) => {
                    match transport.is_connection_oriented() {
                        true => {
                            disconnections += 1;
                            assert!(clients.remove(&endpoint));
                            if disconnections == expected_clients {
                                assert_eq!(expected_clients, messages_received);
                                assert_eq!(0, clients.len());
                                node.stop() //Exit from thread.
                            }
                        }
                        false => unreachable!(),
                    }
                }
            },
        });
    });

    let server_addr = rx.recv_timeout(*TIMEOUT).expect(TIMEOUT_EVENT_RECV_ERR);
    (thread, server_addr)
}

fn start_echo_client_manager(
    transport: Transport,
    server_addr: SocketAddr,
    clients_number: usize,
) -> NamespacedThread<()> {
    NamespacedThread::spawn("test-client", move || {
        let (node, listener) = node::split();
        node.signals().send_with_timer((), *TIMEOUT);

        let mut clients = HashSet::new();
        let mut received = 0;

        for _ in 0..clients_number {
            node.network().connect(transport, server_addr).unwrap();
        }

        listener.for_each(move |event| match event {
            NodeEvent::Signal(_) => panic!("{}", TIMEOUT_EVENT_RECV_ERR),
            NodeEvent::Network(net_event) => match net_event {
                NetEvent::Connected(server, status) => {
                    assert!(status);
                    let status = node.network().send(server, MIN_MESSAGE);
                    assert_eq!(SendStatus::Sent, status);
                    assert!(clients.insert(server));
                }
                NetEvent::Message(endpoint, data) => {
                    assert!(clients.remove(&endpoint));
                    assert_eq!(MIN_MESSAGE, data);
                    node.network().remove(endpoint.resource_id());

                    received += 1;
                    if received == clients_number {
                        node.stop(); //Exit from thread.
                    }
                }
                NetEvent::Accepted(..) => unreachable!(),
                NetEvent::Disconnected(_) => unreachable!(),
            },
        });
    })
}

fn start_burst_receiver(
    transport: Transport,
    expected_count: usize,
) -> (NamespacedThread<()>, SocketAddr) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let thread = NamespacedThread::spawn("test-receiver", move || {
        let (node, listener) = node::split();
        node.signals().send_with_timer((), *TIMEOUT);

        let (_, receiver_addr) = node.network().listen(transport, LOCAL_ADDR).unwrap();
        tx.send(receiver_addr).unwrap();

        let mut count = 0;
        listener.for_each(move |event| match event {
            NodeEvent::Signal(_) => panic!("{}", TIMEOUT_EVENT_RECV_ERR),
            NodeEvent::Network(net_event) => match net_event {
                NetEvent::Connected(..) => unreachable!(),
                NetEvent::Accepted(..) => (),
                NetEvent::Message(_, data) => {
                    let expected_message = format!("{}: {}", SMALL_MESSAGE, count);
                    assert_eq!(expected_message, String::from_utf8_lossy(&data));
                    count += 1;
                    if count == expected_count {
                        node.stop();
                    }
                }
                NetEvent::Disconnected(_) => (),
            },
        });
    });
    (thread, rx.recv().unwrap())
}

fn start_burst_sender(
    transport: Transport,
    receiver_addr: SocketAddr,
    expected_count: usize,
) -> NamespacedThread<()> {
    NamespacedThread::spawn("test-sender", move || {
        let (node, listener) = node::split::<()>();

        let (receiver, _) = node.network().connect(transport, receiver_addr).unwrap();

        let mut count = 0;
        listener.for_each(move |event| match event {
            NodeEvent::Signal(_) => {
                if count < expected_count {
                    let message = format!("{}: {}", SMALL_MESSAGE, count);
                    let status = node.network().send(receiver, message.as_bytes());
                    assert_eq!(SendStatus::Sent, status);

                    count += 1;
                    if !transport.is_connection_oriented() {
                        // We need a rate to not lose packet.
                        node.signals().send_with_timer((), Duration::from_micros(50));
                    }
                    else {
                        node.signals().send(());
                    }
                }
                else {
                    node.stop();
                }
            }
            NodeEvent::Network(net_event) => match net_event {
                NetEvent::Connected(_, status) => {
                    assert!(status);
                    node.signals().send(());
                }
                NetEvent::Disconnected(_) => (),
                _ => unreachable!(),
            },
        });
    })
}

#[cfg_attr(feature = "tcp", test_case(Transport::Tcp, 1))]
#[cfg_attr(feature = "tcp", test_case(Transport::Tcp, 100))]
#[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp, 1))]
#[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp, 100))]
#[cfg_attr(feature = "udp", test_case(Transport::Udp, 1))]
#[cfg_attr(feature = "udp", test_case(Transport::Udp, 100))]
#[cfg_attr(feature = "websocket", test_case(Transport::Ws, 1))]
#[cfg_attr(feature = "websocket", test_case(Transport::Ws, 100))]
// NOTE: A medium-high `clients` value can exceeds the "open file" limits of an OS in CI
// with an obfuscated error message.
fn echo(transport: Transport, clients: usize) {
    //util::init_logger(LogThread::Enabled); // Enable it for better debugging

    let (_server_thread, server_addr) = start_echo_server(transport, clients);
    let _client_thread = start_echo_client_manager(transport, server_addr, clients);
}

// Tcp: Does not apply: it's stream based
#[cfg_attr(feature = "udp", test_case(Transport::Udp, 2000))]
#[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp, 200000))]
#[cfg_attr(feature = "websocket", test_case(Transport::Ws, 200000))]
fn burst(transport: Transport, messages_count: usize) {
    //util::init_logger(LogThread::Enabled); // Enable it for better debugging

    let (_receiver_thread, server_addr) = start_burst_receiver(transport, messages_count);
    let _sender_thread = start_burst_sender(transport, server_addr, messages_count);
}

#[cfg_attr(feature = "tcp", test_case(Transport::Tcp, BIG_MESSAGE_SIZE))]
#[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp, BIG_MESSAGE_SIZE))]
#[cfg_attr(feature = "udp", test_case(Transport::Udp, udp::MAX_LOCAL_PAYLOAD_LEN))]
#[cfg_attr(feature = "websocket", test_case(Transport::Ws, BIG_MESSAGE_SIZE))]
fn message_size(transport: Transport, message_size: usize) {
    //util::init_logger(LogThread::Disabled); // Enable it for better debugging

    assert!(message_size <= transport.max_message_size());

    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let sent_message: Vec<u8> = (0..message_size).map(|_| rng.gen()).collect();

    let (node, listener) = node::split();
    node.signals().send_with_timer((), *TIMEOUT);

    let (_, receiver_addr) = node.network().listen(transport, LOCAL_ADDR).unwrap();
    let (receiver, _) = node.network().connect(transport, receiver_addr).unwrap();

    let mut _async_sender: Option<NamespacedThread<()>> = None;
    let mut received_message = Vec::new();

    listener.for_each(move |event| match event {
        NodeEvent::Signal(_) => panic!("{}", TIMEOUT_EVENT_RECV_ERR),
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Connected(endpoint, status) => {
                assert!(status);
                assert_eq!(receiver, endpoint);

                let node = node.clone();
                let sent_message = sent_message.clone();

                // Protocols as TCP blocks the sender if the receiver is not reading data
                // and its buffer is fill.
                _async_sender = Some(NamespacedThread::spawn("test-sender", move || {
                    let status = node.network().send(receiver, &sent_message);
                    assert_eq!(status, SendStatus::Sent);
                    assert!(node.network().remove(receiver.resource_id()));
                }));
            }
            NetEvent::Accepted(..) => (),
            NetEvent::Message(_, data) => {
                if transport.is_packet_based() {
                    received_message = data.to_vec();
                    assert_eq!(sent_message, received_message);
                    node.stop();
                }
                else {
                    received_message.extend_from_slice(&data);
                }
            }
            NetEvent::Disconnected(_) => {
                assert_eq!(sent_message.len(), received_message.len());
                assert_eq!(sent_message, received_message);
                node.stop();
            }
        },
    });
}

#[test]
fn multicast_reuse_addr() {
    //util::init_logger(LogThread::Disabled); // Enable it for better debugging

    let (node, listener) = node::split();
    node.signals().send_with_timer((), *TIMEOUT_SMALL);

    let multicast_addr = "239.255.0.1:3015";
    let (id_1, _) = node.network().listen(Transport::Udp, multicast_addr).unwrap();
    let (id_2, _) = node.network().listen(Transport::Udp, multicast_addr).unwrap();

    let (target, local_addr) = node.network().connect(Transport::Udp, multicast_addr).unwrap();

    let mut received = 0;
    listener.for_each(move |event| match event {
        NodeEvent::Signal(_) => panic!("{}", TIMEOUT_EVENT_RECV_ERR),
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Connected(endpoint, status) => {
                assert!(status);
                assert_eq!(endpoint, target);
                let status = node.network().send(target, &[42]);
                assert_eq!(status, SendStatus::Sent);
            }
            NetEvent::Message(endpoint, data) => {
                if endpoint.resource_id() != id_1 {
                    assert_eq!(endpoint.resource_id(), id_2);
                }
                if endpoint.resource_id() != id_2 {
                    assert_eq!(endpoint.resource_id(), id_1);
                }
                assert_eq!(data, [42]);
                assert_eq!(endpoint.addr(), local_addr);

                received += 1;
                if received == 2 {
                    node.stop();
                }
            }
            NetEvent::Accepted(..) => unreachable!(),
            NetEvent::Disconnected(_) => unreachable!(),
        },
    });
}
