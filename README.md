[![message-io](https://img.shields.io/crates/v/message-io)](https://crates.io/crates/message-io)
[![license](https://img.shields.io/crates/l/message-io)](https://www.apache.org/licenses/LICENSE-2.0.txt)
[![downloads](https://img.shields.io/crates/d/message-io)](https://crates.io/crates/message-io)

# `message-io`
`message-io` is an asynchronous network message library for building network applications following the actor model.
The library offers an event-based API over an abstraction network transport layer.

This library can be used but it is still growing, so if you see any bug or strange behaviour, please put an issue!
Of course, any contribution is welcome!

## For who is this project?
- People who want to make an application that needs to communicate over tcp/udp protocols.
- People who want to make a multiplayer game (server and/or client).
- People who don't want to deal with concurrence or socket connection problems.
- People who want to push the effort in the messages among the apps, not in how to transport them.

## Features
- Asynchronous: internal poll event with non-blocking sockets.
- TCP, UDP (with multicast option), protocols.
- FIFO events with internal timed and priority events.
- Really easy API:
  - Abstraction from transport layer: Do not thinks about sockets, only thing about data messages.
  - Only two main entities: an extensible event-queue to manage all events.
    and a network manager to manage the connections, and send/receive data.
  - Forget concurrence problem: "One thread to rule them all",
- Significant performance: one thread for manage all internal conenctions over a OS poll, binary serialization, small overhead over OS sockets).

## Getting started
Add to your `Cargo.toml`
```
message-io = "0.3"
```

### Documentation
- [Basic concepts](#basic-concepts)
- [API documentation](https://docs.rs/message-io/)
- Examples:
  - [TCP client and server](examples/tcp)
  - [Basic client server](examples/basic)
  - [Distributed network with discovery server](examples/distributed)
  - [Multicast](examples/multicast)

### Minimal TCP/UDP server
The following example is the simplest server that reads message from a client and respond to it.
It is capable to manage several client connections and listen from 2 differents ports and interfaces.

```rust
use message_io::events::{EventQueue};
use message_io::network::{NetworkManager, NetEvent};

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum Message {
    HelloServer,
    HelloClient,
    // Other messages here
}

enum Event {
    Network(NetEvent<Message>),
    // Other user events here
}

fn main() {
    let mut event_queue = EventQueue::new();

    // Create NetworkManager, the callback will push the network event into the event queue
    let sender = event_queue.sender().clone();
    let mut network = NetworkManager::new(move |net_event| sender.send(Event::Network(net_event)));

    // Listen TCP and UDP messages on ports 3001 and 3002.
    network.listen_tcp("0.0.0.0:3001").unwrap();
    network.listen_udp("0.0.0.0:3002").unwrap();

    loop {
        match event_queue.receive() { // Read the next event or wait until have it.
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    Message::HelloServer => network.send(endpoint, Message::HelloClient).unwrap(),
                    _ => (), // Other messages here
                },
                NetEvent::AddedEndpoint(_endpoint, _address) => println!("TCP Client connected"),
                NetEvent::RemovedEndpoint(_endpoint) => println!("TCP Client disconnected"),
            },
            // Other events here
        }
    }
}
```

## Basic concepts
The library has two main pieces:

- **`EventQueue`**:
Is a generic and synchronized queue where all the system events are sent.
The user must be read these events in its main thread in order to dispatch actions.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vQr06OL40IWagXWHoyytUIlR1SHoahYE0Pkj6r0HmokaUMW4ojC5MV2OViFO9m-2jDqrDokPJ62oSzg/pub?w=837&h=313"/>
</p>

- **`NetworkManager`**:
It is an abstraction layer of the transport protocols that works over non-blocking sockets.
It allows to create/remove connections, send and receive messages (defined by the user).

To manage the connections the `NetworkManager` offers an *`Endpoint`* that is an unique identifier of the connection
that can be used to remove, send or identify input messages.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vS3y1BKwPHjoFqtHm2pqfmvxr0JRQIzeRJim9s2UOrOIS74cGwlyqxnH4_DHVXTverziCjPzl6FtQMe/pub?w=586&h=273"/>
</p>

The power comes when both pieces joins together, allowing to process all actions from one thread.
To reach this, the user have to connect the `NetworkManager` to the `EventQueue` sending `NetEvent` produced by the first one.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vT6IuBVr4mLbdNfs2yZayqqUJ04PsuqG27Ce3Vdr0ZG8ItX3slISoKVxyndybaYPIS5oFZ6N4TljrKQ/pub?w=701&h=383"/>
</p>

## Test yourself!
Clone the repository and test the `basic`example that you can found in [`examples/basic`](examples/basic):

Run the server:
```
cargo run --example basic server [tcp/udp]
```
In other terminals, run one or more clients:
```
cargo run --example basic client [tcp/udp]
```
(By default, if no protocol is specified, `tcp` is used)

