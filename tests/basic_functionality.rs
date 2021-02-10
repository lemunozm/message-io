use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Transport, SendStatus};
use message_io::{MAX_UDP_LEN};

use std::net::{TcpStream, Shutdown};
use std::time::{Duration};
use std::sync::{Once};
use simple_logger::{SimpleLogger};

const SMALL_MESSAGE: &'static str = "Small message";
const TIMEOUT: u64 = 5; //seconds
const LOCAL_ADDR: &'static str = "127.0.0.1:0";

// Used to init the log only one time for all tests;
static INIT: Once = Once::new();

#[test]
fn simple_connection_data_disconnection_by_tcp() {
    INIT.call_once(|| SimpleLogger::new().init().unwrap());

    let mut event_queue = EventQueue::<NetEvent<String>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    let (_, server_addr) = network.listen(Transport::Tcp, LOCAL_ADDR).unwrap();

    let server_handle = std::thread::spawn(move || {
        let mut client_endpoint = None;
        let _ = network;
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(*client_endpoint.as_ref().unwrap(), endpoint);
                    assert_eq!(message, SMALL_MESSAGE);
                    let status = network.send(endpoint, message);
                    assert_eq!(status, SendStatus::Sent);
                }
                NetEvent::AddedEndpoint(endpoint) => {
                    assert!(client_endpoint.is_none());
                    client_endpoint = Some(endpoint);
                }
                NetEvent::RemovedEndpoint(endpoint) => {
                    assert_eq!(client_endpoint.take().unwrap(), endpoint);
                    break //Exit from thread, the connection will be automatically close
                }
                NetEvent::DeserializationError(_) => unreachable!(),
            }
        }
        assert!(client_endpoint.is_none());
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::<NetEvent<String>>::new();
        let sender = event_queue.sender().clone();
        let mut network = Network::new(move |net_event| sender.send(net_event));

        let server_endpoint = network.connect(Transport::Tcp, server_addr).unwrap();
        let status = network.send(server_endpoint, SMALL_MESSAGE.to_string());
        assert_eq!(status, SendStatus::Sent);
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(server_endpoint, endpoint);
                    assert_eq!(message, SMALL_MESSAGE);
                    break //Exit from thread, the connection will be automatically close
                }
                NetEvent::AddedEndpoint(endpoint) => {
                    assert_eq!(server_endpoint, endpoint);
                }
                NetEvent::RemovedEndpoint(_) => unreachable!(),
                NetEvent::DeserializationError(_) => unreachable!(),
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

#[test]
fn simple_data_by_udp() {
    INIT.call_once(|| SimpleLogger::new().init().unwrap());

    let mut event_queue = EventQueue::<NetEvent<String>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    let (upd_listen_resource_id, server_addr) = network.listen(Transport::Udp, LOCAL_ADDR).unwrap();

    let server_handle = std::thread::spawn(move || {
        let _ = network;
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(endpoint, message) => {
                    log::info!("from server: {}", endpoint);
                    assert_eq!(upd_listen_resource_id, endpoint.resource_id());
                    assert_eq!(message, SMALL_MESSAGE);
                    let status = network.send(endpoint, message);
                    assert_eq!(status, SendStatus::Sent);
                    break //Exit from thread
                }
                _ => unreachable!(),
            }
        }
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::<NetEvent<String>>::new();
        let sender = event_queue.sender().clone();
        let mut network = Network::new(move |net_event| sender.send(net_event));

        let server_endpoint = network.connect(Transport::Udp, server_addr).unwrap();
        let status = network.send(server_endpoint, SMALL_MESSAGE.to_string());
        assert_eq!(status, SendStatus::Sent);
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(endpoint, message) => {
                    log::info!("from client: {}", endpoint);
                    assert_eq!(server_endpoint, endpoint);
                    assert_eq!(message, SMALL_MESSAGE);
                    break //Exit from thread
                }
                _ => unreachable!(),
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

#[test]
fn long_tcp_message() {
    INIT.call_once(|| SimpleLogger::new().init().unwrap());

    let mut event_queue = EventQueue::<NetEvent<Vec<u8>>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    const MESSAGE_SIZE: usize = 1_000_000; // Arround 1MB
    const VALUE: u8 = 0xAA;
    let (_, receiver_addr) = network.listen(Transport::Tcp, LOCAL_ADDR).unwrap();

    let receiver_handle = std::thread::spawn(move || {
        // Pass the network to the thread. The network should be destroyed before event queue.
        let _ = network;
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(_, message) => {
                    assert_eq!(message.len(), MESSAGE_SIZE);
                    assert!(message.iter().all(|&byte| byte == VALUE));
                    break
                }
                NetEvent::AddedEndpoint(_) => (),
                NetEvent::RemovedEndpoint(_) => (),
                NetEvent::DeserializationError(_) => unreachable!(),
            }
        }
    });

    let mut event_queue = EventQueue::<NetEvent<Vec<u8>>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    let receiver = network.connect(Transport::Tcp, receiver_addr).unwrap();
    let message = std::iter::repeat(VALUE).take(MESSAGE_SIZE).collect::<Vec<_>>();
    let status = network.send(receiver, message.clone());
    assert_eq!(status, SendStatus::Sent);

    receiver_handle.join().unwrap();
}

#[test]
fn max_udp_size_message() {
    INIT.call_once(|| SimpleLogger::new().init().unwrap());

    let mut event_queue = EventQueue::<NetEvent<Vec<u8>>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    const MESSAGE_SIZE: usize = MAX_UDP_LEN - 8; // Vec<u8> header + encoding header
    const VALUE: u8 = 0xFF;
    let (_, receiver_addr) = network.listen(Transport::Udp, LOCAL_ADDR).unwrap();

    let receiver_handle = std::thread::spawn(move || {
        // Pass the network to the thread. The network should be destroyed before event queue.
        let _ = network;
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(_, message) => {
                    assert_eq!(message.len(), MESSAGE_SIZE);
                    assert!(message.iter().all(|&byte| byte == VALUE));
                    break
                }
                NetEvent::AddedEndpoint(_) => (),
                NetEvent::RemovedEndpoint(_) => (),
                NetEvent::DeserializationError(_) => unreachable!(),
            }
        }
    });

    let mut event_queue = EventQueue::<NetEvent<Vec<u8>>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    let receiver = network.connect(Transport::Udp, receiver_addr).unwrap();
    let message = std::iter::repeat(VALUE).take(MESSAGE_SIZE).collect::<Vec<_>>();
    let status = network.send(receiver, message);
    assert_eq!(status, SendStatus::Sent);

    receiver_handle.join().unwrap();
}

#[test]
fn disconnection() {
    INIT.call_once(|| SimpleLogger::new().init().unwrap());

    let mut event_queue = EventQueue::<NetEvent<Vec<u8>>>::new();
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(net_event));

    let (_, receiver_addr) = network.listen(Transport::Tcp, LOCAL_ADDR).unwrap();

    let receiver_handle = std::thread::spawn(move || {
        // Pass the network to the thread. The network should be destroyed before event queue.
        let _ = network;
        let mut connected = false;
        loop {
            match event_queue.receive_timeout(Duration::from_secs(TIMEOUT)).unwrap() {
                NetEvent::Message(..) => unreachable!(),
                NetEvent::AddedEndpoint(_) => connected = true,
                NetEvent::RemovedEndpoint(_) => {
                    assert_eq!(connected, true);
                    break
                }
                NetEvent::DeserializationError(_) => unreachable!(),
            }
        }
    });

    let stream = TcpStream::connect(receiver_addr).unwrap();
    stream.shutdown(Shutdown::Both).unwrap();

    receiver_handle.join().unwrap();
}
