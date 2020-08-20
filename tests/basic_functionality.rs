use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use serde::{Serialize, Deserialize};

const MESSAGE_DATA: &'static str = "Small message";

#[derive(Serialize, Deserialize)]
enum Message {
    Data(String),
}

enum Event {
    Network(NetEvent<Message>),
}

#[test]
fn simple_connection_data_disconnection_by_tcp() {
    let mut event_queue = EventQueue::new();
    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let mut client_endpoint = None;

    let (_, server_addr) = network.listen_tcp("127.0.0.1:0").unwrap();

    let server_handle = std::thread::spawn(move || {
        loop {
            match event_queue.receive() {
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(endpoint, message) => match message {
                        Message::Data(text) => {
                            assert_eq!(*client_endpoint.as_ref().unwrap(), endpoint);
                            assert_eq!(text, MESSAGE_DATA);
                            network.send(endpoint, Message::Data(text)).unwrap();
                        }
                    },
                    NetEvent::AddedEndpoint(endpoint) => {
                        assert!(client_endpoint.is_none());
                        client_endpoint = Some(endpoint);
                    },
                    NetEvent::RemovedEndpoint(endpoint) => {
                        assert_eq!(client_endpoint.unwrap(), endpoint);
                        break //Exit from thread, the connection will be automatically close
                    },
                },
            }
        }
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::new();
        let network_sender = event_queue.sender().clone();
        let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

        let server_endpoint = network.connect_tcp(server_addr).unwrap();
        network.send(server_endpoint, Message::Data(MESSAGE_DATA.into())).unwrap();
        loop {
            match event_queue.receive() {
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(endpoint, message) => match message {
                        Message::Data(text) => {
                            assert_eq!(server_endpoint, endpoint);
                            assert_eq!(text, MESSAGE_DATA);
                            network.send(endpoint, Message::Data(text)).unwrap();
                            break //Exit from thread, the connection will be automatically close
                        }
                    },
                    NetEvent::AddedEndpoint(endpoint) => {
                        assert_eq!(server_endpoint, endpoint);
                    },
                    NetEvent::RemovedEndpoint(_) => unreachable!()
                },
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();

    assert!(client_endpoint.is_none());
}

#[test]
fn simple_data_by_udp() {
    let mut event_queue = EventQueue::new();
    let network_sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

    let (upd_listen_resource_id, server_addr) = network.listen_udp("127.0.0.1:0").unwrap();

    let server_handle = std::thread::spawn(move || {
        loop {
            match event_queue.receive() {
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(endpoint, message) => match message {
                        Message::Data(text) => {
                            assert_eq!(upd_listen_resource_id, endpoint.resource_id());
                            assert_eq!(text, MESSAGE_DATA);
                            network.send(endpoint, Message::Data(text)).unwrap();
                            println!("server off");
                            break; //Exit from thread
                        }
                    },
                    _ => unreachable!()
                },
            }
        }
    });

    let client_handle = std::thread::spawn(move || {
        let mut event_queue = EventQueue::new();
        let network_sender = event_queue.sender().clone();
        let mut network = NetworkManager::new(move |net_event| network_sender.send(Event::Network(net_event)));

        let server_endpoint = network.connect_udp(server_addr).unwrap();
        network.send(server_endpoint, Message::Data(MESSAGE_DATA.into())).unwrap();
        loop {
            match event_queue.receive() {
                Event::Network(net_event) => match net_event {
                    NetEvent::Message(endpoint, message) => match message {
                        Message::Data(text) => {
                            assert_eq!(server_endpoint, endpoint);
                            assert_eq!(text, MESSAGE_DATA);
                            println!("client off");
                            break //Exit from thread
                        }
                    },
                    _ => unreachable!()
                },
            }
        }
    });

    server_handle.join().unwrap();
    client_handle.join().unwrap();
}
