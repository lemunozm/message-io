use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

const MESSAGE_DATA: &'static str = "Small message";

#[test]
fn simple_connection_data_disconnection_by_tcp() {
    let mut event_queue = EventQueue::<NetEvent<String>>::new();
    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(net_event));

    let (_, server_addr) = network.listen_tcp("127.0.0.1:0").unwrap();

    let server_handle = std::thread::spawn(move || {
        let mut client_endpoint = None;
        loop {
            match event_queue.receive() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(*client_endpoint.as_ref().unwrap(), endpoint);
                    assert_eq!(message, MESSAGE_DATA);
                    network.send(endpoint, message).unwrap();
                },
                NetEvent::AddedEndpoint(endpoint) => {
                    assert!(client_endpoint.is_none());
                    client_endpoint = Some(endpoint);
                }
                NetEvent::RemovedEndpoint(endpoint) => {
                    assert_eq!(client_endpoint.take().unwrap(), endpoint);
                    break //Exit from thread, the connection will be automatically close
                }
            }
        }
        assert!(client_endpoint.is_none());
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::<NetEvent<String>>::new();
        let sender = event_queue.sender().clone();
        let mut network = NetworkManager::new(move |net_event| sender.send(net_event));

        let server_endpoint = network.connect_tcp(server_addr).unwrap();
        network.send(server_endpoint, MESSAGE_DATA.to_string()).unwrap();
        loop {
            match event_queue.receive() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(server_endpoint, endpoint);
                    assert_eq!(message, MESSAGE_DATA);
                    network.send(endpoint, message).unwrap();
                    break //Exit from thread, the connection will be automatically close
                },
                NetEvent::AddedEndpoint(endpoint) => {
                    assert_eq!(server_endpoint, endpoint);
                }
                NetEvent::RemovedEndpoint(_) => unreachable!(),
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}

#[test]
fn simple_data_by_udp() {
    let mut event_queue = EventQueue::<NetEvent<String>>::new();
    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(net_event));

    let (upd_listen_resource_id, server_addr) = network.listen_udp("127.0.0.1:0").unwrap();

    let server_handle = std::thread::spawn(move || {
        loop {
            match event_queue.receive() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(upd_listen_resource_id, endpoint.resource_id());
                    assert_eq!(message, MESSAGE_DATA);
                    network.send(endpoint, message).unwrap();
                    break //Exit from thread
                },
                _ => unreachable!(),
            }
        }
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::<NetEvent<String>>::new();
        let sender = event_queue.sender().clone();
        let mut network = NetworkManager::new(move |net_event| sender.send(net_event));

        let server_endpoint = network.connect_udp(server_addr).unwrap();
        network.send(server_endpoint, MESSAGE_DATA.to_string()).unwrap();
        loop {
            match event_queue.receive() {
                NetEvent::Message(endpoint, message) => {
                    assert_eq!(server_endpoint, endpoint);
                    assert_eq!(message, MESSAGE_DATA);
                    break //Exit from thread
                },
                _ => unreachable!(),
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}


