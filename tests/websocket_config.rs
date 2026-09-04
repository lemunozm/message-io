#![cfg(feature = "websocket")]

use message_io::adapters::ws::WsListenConfig;
use message_io::network::NetEvent;
use message_io::node::{self, NodeEvent};
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;
use tungstenite::Message;
use tungstenite::protocol::frame::Frame;
use tungstenite::protocol::frame::coding::{Data, OpCode};

const MESSAGE_LIMIT: usize = 64;

fn configured_listener(
) -> (message_io::node::NodeHandler<()>, message_io::node::NodeListener<()>, SocketAddr) {
    let (handler, listener) = node::split();
    let config = WsListenConfig::default()
        .with_max_frame_size(Some(MESSAGE_LIMIT))
        .with_max_message_size(Some(MESSAGE_LIMIT));
    let (_, address) = handler.network().listen_ws_with(config, "127.0.0.1:0").unwrap();
    (handler, listener, address)
}

fn send(address: SocketAddr, messages: Vec<Message>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let (mut socket, _) =
            tungstenite::connect(format!("ws://{address}/message-io-default")).unwrap();
        for message in messages {
            let _ = socket.send(message);
        }
    })
}

#[test]
fn configured_listener_accepts_a_message_at_the_limit() {
    let (handler, listener, address) = configured_listener();
    handler.signals().send_with_timer((), Duration::from_secs(2));
    let client = send(address, vec![Message::binary(vec![0; MESSAGE_LIMIT])]);

    listener.for_each(move |event| match event {
        NodeEvent::Network(NetEvent::Message(_, bytes)) => {
            assert_eq!(bytes.len(), MESSAGE_LIMIT);
            handler.stop();
        }
        NodeEvent::Network(NetEvent::Disconnected(_)) => {
            panic!("the exact-limit message was disconnected")
        }
        NodeEvent::Signal(()) => panic!("timed out waiting for the exact-limit message"),
        NodeEvent::Network(_) => {}
    });
    client.join().unwrap();
}

fn assert_disconnected(messages: Vec<Message>) {
    let (handler, listener, address) = configured_listener();
    handler.signals().send_with_timer((), Duration::from_secs(2));
    let client = send(address, messages);

    listener.for_each(move |event| match event {
        NodeEvent::Network(NetEvent::Message(_, _)) => {
            panic!("an oversized WebSocket message reached the application")
        }
        NodeEvent::Network(NetEvent::Disconnected(_)) => handler.stop(),
        NodeEvent::Signal(()) => panic!("timed out waiting for an oversized-message disconnect"),
        NodeEvent::Network(_) => {}
    });
    client.join().unwrap();
}

#[test]
fn configured_listener_rejects_oversized_binary_and_text_frames() {
    assert_disconnected(vec![Message::binary(vec![0; MESSAGE_LIMIT + 1])]);
    assert_disconnected(vec![Message::text("x".repeat(MESSAGE_LIMIT + 1))]);
}

#[test]
fn configured_listener_rejects_an_oversized_fragmented_message() {
    assert_disconnected(vec![
        Message::Frame(Frame::message(vec![0; 40], OpCode::Data(Data::Binary), false)),
        Message::Frame(Frame::message(vec![0; 40], OpCode::Data(Data::Continue), true)),
    ]);
}
