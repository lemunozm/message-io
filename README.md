[![](https://img.shields.io/crates/v/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/docsrs/message-io)](https://docs.rs/message-io)
[![](https://img.shields.io/crates/l/message-io)](https://www.apache.org/licenses/LICENSE-2.0.txt)
[![](https://img.shields.io/crates/d/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/github/workflow/status/lemunozm/message-io/message-io%20ci)](https://github.com/lemunozm/message-io/actions?query=workflow%3A%22message-io+ci%22)


# message-io
`message-io` is an event-driven message library to build network applications **easy** and **fast**.
The library handles the internal OS socket in order to offer a simple event message API to the user.
It also allows you to make an adapter for your own transport protocol following some
[rules](#custom-adapter), delegating to the library the tedious asynchrony and thread management.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vSPmycMsWoQq60MPEODcakFQVPkDwVy98AnduTswFNPGBB5dpbIsSCHHBhS2iEuSUtbVaYQb7zgfgjO/pub?w=653&h=305" width="653"/>
</p>

If you find a problem using the library or you have an improvement idea,
do not hesitate to open an issue. **Any contribution is welcome!**

## Motivation
Managing sockets is hard because you need to fight with threads, concurrency, full duplex, encoding,
IO errors that come from the OS (which are really difficult to understand in some situations), etc.
If you make use of *non-blocking* sockets, it adds a new layer of complexity:
synchronize the events that come asynchronously from the Operative System.

`message-io` offers an easy way to deal with all these mentioned problems,
making them transparently for you,
the programmer that wants to make an application with its own problems.
For that, the library gives you a simple API with two concepts to understand:
**messages** (the data you send and receive), and **endpoints** (the recipients of that data).
This abstraction also offers the possibility to use the same API independently
of the transport protocol used.
You could change the transport of your application in literally one line.

## Features
- Highly scalable: **non-blocking sockets** that allows to manage thousands of active connections.
- Multiplatform: see [mio platform support](https://github.com/tokio-rs/mio#platforms).
- Multiples transports
([docs](https://docs.rs/message-io/latest/message_io/network/enum.Transport.html)):
  - **TCP**: stream and framed mode (to deal with messages instead of stream)
  - **UDP**, with multicast option
  - **WebSocket**: basic and secure option using
    [tungstenite-rs](https://github.com/snapview/tungstenite-rs).
- Custom FIFO events with timers and priority.
- Easy, intuitive and consistent API:
  - Follows [KISS principle](https://en.wikipedia.org/wiki/KISS_principle).
  - Abstraction from transport layer: do not think about sockets, think about messages and endpoints.
  - Only two main entities to use:
    - a [`NodeHandler`](https://docs.rs/message-io/latest/message_io/node/struct.NodeHandler.html)
    to manage all connections (connect, listen, remove, send) and signals (timers, priority).
    - a [`NodeListener`](https://docs.rs/message-io/latest/message_io/node/struct.NodeListener.html)
    to process all signals and events from the network.
  - Forget concurrence problems: handle all connection and listeners from one thread:
    "One thread to rule them all".
  - Easy error handling:
    do not deal with dark internal `std::io::Error` when send/receive from the network.
- High performance (see the [benchmarks](docs/performance_benchmarks.md)):
    - Write/read messages with zero-copy.
    You write and read directly from the internal OS socket buffer without any copy in the middle by the library.
    - Full duplex: simultaneous reading/writing operations over same internal OS socket.
- Customizable: `message-io` doesn't have the transport you need?
  Add easily and [adapter](#custom-adapter).

## Getting started
Add to your `Cargo.toml` (all the transports included by default):
```toml
[dependencies]
message-io = "0.12"
```
If you **only** want to use a subset of the available transport battery,
you can select them by their associated features `tcp`, `udp`, and `websocket`.
For example, in order to include only *TCP* and *UDP*, add to your `Cargo.toml`:
```toml
[dependencies]
message-io = { version = "0.12", default-features = false, features = ["tcp", "udp"] }
```

**Warning**: Version **0.12** comes with important API changes ([changelog](CHANGELOG.md))
in order to reach [zero-copy write/read](https://github.com/lemunozm/message-io/issues/61) goal.
If you find problems porting your application to this version,
check the examples folder, API docs, and don't hesitate to open an issue.

### Documentation
- [API documentation](https://docs.rs/message-io/)
- [Basic concepts](docs/basic_concepts.md)
- [Examples](examples):
  - [Ping Pong](examples/ping-pong) (a simple client/server example)
  - [Multicast](examples/multicast)
  - [Distributed network with discovery server](examples/distributed)
  - [File transfer](examples/file-transfer)

### All in one: TCP, UDP and WebSocket echo server
The following example is the simplest server that reads messages from the clients and responds
to them.
It is capable to manage several client connections and listen from 3 differents protocols
at the same time.

```rust,no_run
use message_io::node::{self};
use message_io::network::{NetEvent, Transport};

fn main() {
    // Create a node, the main message-io entity. It is divided in 2 parts:
    // The 'handler', used to make actions (connect, send messages, signals, stop the node...)
    // The 'listener', used to read events from the network or signals.
    let (handler, listener) = node::split::<()>();

    // Listen for TCP, UDP and WebSocket messages at the same time.
    handler.network().listen(Transport::FramedTcp, "0.0.0.0:3042").unwrap();
    handler.network().listen(Transport::Udp, "0.0.0.0:3043").unwrap();
    handler.network().listen(Transport::Ws, "0.0.0.0:3044").unwrap();

    // Read incoming network events.
    listener.for_each(move |event| match event.network() {
        NetEvent::Connected(_endpoint, _) => println!("Client connected"), // Tcp or Ws
        NetEvent::Message(endpoint, data) => {
            println!("Received: {}", String::from_utf8_lossy(data));
            handler.network().send(endpoint, data);
        },
        NetEvent::Disconnected(_endpoint) => println!("Client disconnected"), //Tcp or Ws
    });
}
```

### Echo client
The following example shows a client that can connect to the previous server.
It sends a message each second to the server and listen its echo response.
Changing the `Transport::FramedTcp` to `Udp` or `Ws` will change the underlying transport used.
You can create the number of connections you want at the same time, without any extra thread.

```rust,no_run
use message_io::node::{self, NodeEvent};
use message_io::network::{NetEvent, Transport};
use std::time::Duration;

enum Signal {
    Greet,
    // Any other app event here.
}

fn main() {
    let (handler, listener) = node::split();

    // You can change the transport to Udp or Ws (WebSocket).
    let (server, _) = handler.network().connect(Transport::FramedTcp, "127.0.0.1:3042").unwrap();

    handler.signals().send(Signal::Greet); // Start sending

    listener.for_each(move |event| match event {
        NodeEvent::Signal(signal) => match signal {
            Signal::Greet => { // computed every second
                handler.network().send(server, "Hello server!".as_bytes());
                handler.signals().send_with_timer(Signal::Greet, Duration::from_secs(1));
            }
        }
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Message(_endpoint, data) => {
                println!("Received: {}", String::from_utf8_lossy(data));
            },
            _ => unreachable!(), // Connected and Disconnected are only generated by listening
        }
    });
}
```

## Test it yourself!
Clone the repository and test the *Ping Pong* example
(similar to the *README* example but more vitaminized).

Run the server:
```sh
cargo run --example ping-pong server tcp 3456
```
Run the client:
```sh
cargo run --example ping-pong client tcp 127.0.0.1:3456
```

You can play with it changing the transport, running several clients, disconnect them, etc.
See more [here](examples/ping-pong).

## Do you need a transport protocol that `message-io` doesn't have? Add an adapter! <span id="custom-adapter"><span>

`message-io` offers two *kinds* of APIs.
The **user API**, that talks to `message-io` itself as an user that want to use the library,
and the internal **adapter API** for those who want to add their protocol adapters into the library.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRMwZsL8Tki3Sq9Zc2hpZ8L3bJPuj38zgiMKzBCXsX3wrPnfyG2hp-ijmDFUPqicEQZFeyUFxhcdJMB/pub?w=546&h=276"/>
</p>

If a transport protocol can be built in top of [`mio`](https://github.com/tokio-rs/mio)
(most of the existing protocol libraries can), then you can add it to `message-io` **really easy**:

1. Add your *adapter* file in `src/adapters/<my-transport-protocol>.rs` that implements the
  traits that you find [here](https://docs.rs/message-io/latest/message_io/network/adapter/index.html).
  It contains only 7 mandatory functions to implement (see the [template](src/adapters/template.rs)),
  and take little more than 150 lines to implement an adapter file.

1. Add a new field in the `Transport` enum found in
[src/network/transport.rs](src/network/transport.rs) to register your new adapter.

That's all.
You can use your new transport with the `message-io` API like any other.

Oops! one step more, make a *Pull request* so everyone can use it :)

