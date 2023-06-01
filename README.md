[![](https://img.shields.io/crates/v/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/docsrs/message-io)](https://docs.rs/message-io)
[![](https://img.shields.io/crates/l/message-io)](https://www.apache.org/licenses/LICENSE-2.0.txt)
[![](https://img.shields.io/crates/d/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/github/actions/workflow/status/lemunozm/message-io/.github/workflows/rust.yml?branch=master)](https://github.com/lemunozm/message-io/actions?query=workflow%3A%22message-io+ci%22)
[![](https://img.shields.io/badge/buymeacoffee-donate-yellow)](https://www.buymeacoffee.com/lemunozm)

<p align="center">
  <img src="docs/images/message-io-title.png" title="message-io">
</p>

`message-io` is a fast and easy-to-use event-driven network library.
The library handles the OS socket internally and offers a simple event message API to the user.
It also allows you to make an adapter for your own transport protocol following some
[rules](#custom-adapter), delegating the tedious asynchrony and thread management to the library.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vSPmycMsWoQq60MPEODcakFQVPkDwVy98AnduTswFNPGBB5dpbIsSCHHBhS2iEuSUtbVaYQb7zgfgjO/pub?w=653&h=305" width="653"/>
</p>

If you find a problem using the library or you have an idea to improve it,
do not hesitate to open an issue. **Any contribution is welcome!**
And remember: more [caffeine](https://www.buymeacoffee.com/lemunozm), more productive!

## Motivation
Managing sockets is hard because you need to fight with threads, concurrency, full duplex, encoding,
IO errors that come from the OS (which are really difficult to understand in some situations), etc.
If you make use of *non-blocking* sockets, it adds a new layer of complexity:
synchronize the events that come asynchronously from the Operating System.

`message-io` offers an easy way to deal with all these aforementioned problems,
making them transparent for you,
the programmer that wants to make an application with its own problems.
For that, the library gives you a simple API with two concepts to understand:
**messages** (the data you send and receive), and **endpoints** (the recipients of that data).
This abstraction also offers the possibility to use the same API independently
of the transport protocol used.
You could change the transport of your application in literally one line.

## Features
- Highly scalable: **non-blocking sockets** that allow for the management of thousands of active connections.
- Multiplatform: see [mio platform support](https://github.com/tokio-rs/mio#platforms).
- Multiple transport protocols
([docs](https://docs.rs/message-io/latest/message_io/network/enum.Transport.html)):
  - **TCP**: stream and framed mode (to deal with messages instead of stream)
  - **UDP**, with multicast option
  - **WebSocket**: plain and ~~secure~~[#102](https://github.com/lemunozm/message-io/issues/102)
  option using [tungstenite-rs](https://github.com/snapview/tungstenite-rs)
  (`wasm` is not supported but [planned](https://github.com/lemunozm/message-io/issues/100)).
- Custom FIFO events with timers and priority.
- Easy, intuitive and consistent API:
  - Follows [KISS principle](https://en.wikipedia.org/wiki/KISS_principle).
  - Abstraction from transport layer: don't think about sockets, think about messages and endpoints.
  - Only two main entities to use:
    - a [`NodeHandler`](https://docs.rs/message-io/latest/message_io/node/struct.NodeHandler.html)
    to manage all connections (connect, listen, remove, send) and signals (timers, priority).
    - a [`NodeListener`](https://docs.rs/message-io/latest/message_io/node/struct.NodeListener.html)
    to process all signals and events from the network.
  - Forget concurrency problems: handle all connection and listeners from one thread:
    "One thread to rule them all".
  - Easy error handling:
    do not deal with dark internal `std::io::Error` when sending/receiving from the network.
- High performance (see the [benchmarks](docs/performance_benchmarks.md)):
    - Write/read messages with zero-copy.
    You write and read directly from the internal OS socket buffer without any copy in the middle by the library.
    - Full duplex: simultaneous reading/writing operations over the same internal OS socket.
- Customizable: `message-io` doesn't have the transport you need?
  Easily add an [adapter](#custom-adapter).

## Documentation
- [API documentation](https://docs.rs/message-io/)
- [Basic concepts](docs/basic_concepts.md)
- [Benchmarks](docs/performance_benchmarks.md)
- [Examples](examples):
  - [Ping Pong](examples/ping-pong) (a simple client/server example)
  - [Multicast](examples/multicast)
  - [Distributed network with discovery server](examples/distributed)
  - [File transfer](examples/file-transfer)
- [Open Source applications](#app-list)

## Getting started
Add to your `Cargo.toml` (all transports included by default):
```toml
[dependencies]
message-io = "0.17"
```
If you **only** want to use a subset of the available transport battery,
you can select them by their associated features `tcp`, `udp`, and `websocket`.
For example, in order to include only *TCP* and *UDP*, add to your `Cargo.toml`:
```toml
[dependencies]
message-io = { version = "0.17", default-features = false, features = ["tcp", "udp"] }
```

### All in one: TCP, UDP and WebSocket echo server
The following example is the simplest server that reads messages from the clients and responds
to them with the same message.
It is able to offer the "service" for 3 differents protocols at the same time.

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
        NetEvent::Connected(_, _) => unreachable!(), // Used for explicit connections.
        NetEvent::Accepted(_endpoint, _listener) => println!("Client connected"), // Tcp or Ws
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

    listener.for_each(move |event| match event {
        NodeEvent::Network(net_event) => match net_event {
            NetEvent::Connected(_endpoint, _ok) => handler.signals().send(Signal::Greet),
            NetEvent::Accepted(_, _) => unreachable!(), // Only generated by listening
            NetEvent::Message(_endpoint, data) => {
                println!("Received: {}", String::from_utf8_lossy(data));
            },
            NetEvent::Disconnected(_endpoint) => (),
        }
        NodeEvent::Signal(signal) => match signal {
            Signal::Greet => { // computed every second
                handler.network().send(server, "Hello server!".as_bytes());
                handler.signals().send_with_timer(Signal::Greet, Duration::from_secs(1));
            }
        }
    });
}
```

### Test it yourself!
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

You can play with it by changing the transport, running several clients, disconnecting them, etc.
See more [here](examples/ping-pong).

## Do you need a transport protocol that `message-io` doesn't have? Add an adapter! <span id="custom-adapter"/>

`message-io` offers two *kinds* of API.
The **user API** that talks to `message-io` itself as a user of the library,
and the internal **adapter API** for those who want to add their protocol adapters into the library.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRMwZsL8Tki3Sq9Zc2hpZ8L3bJPuj38zgiMKzBCXsX3wrPnfyG2hp-ijmDFUPqicEQZFeyUFxhcdJMB/pub?w=546&h=276"/>
</p>

If a transport protocol can be built in top of [`mio`](https://github.com/tokio-rs/mio)
(most of the existing protocol libraries can), then you can add it to `message-io` **really easily**:

1. Add your *adapter* file in `src/adapters/<my-transport-protocol>.rs` that implements the
  traits that you find [here](https://docs.rs/message-io/latest/message_io/network/adapter/index.html).
  It contains only 8 mandatory functions to implement (see the [template](src/adapters/template.rs)),
  and it takes arround 150 lines to implement an adapter.

1. Add a new field in the `Transport` enum found in
[src/network/transport.rs](src/network/transport.rs) to register your new adapter.

That's all.
You can use your new transport with the `message-io` API like any other.

Oops! one more step: make a *Pull Request* so everyone can use it :)

## Open source projects using `message-io` <span id="app-list"/>
- [Termchat](https://github.com/lemunozm/termchat) Terminal chat through the LAN with video streaming and file transfer.
- [Egregoria](https://github.com/Uriopass/Egregoria) Contemplative society simulation.
- [Project-Midas](https://github.com/ray33ee/Project-Midas) Distributed network based parallel computing system.
- [AsciiArena](https://github.com/lemunozm/asciiarena) Terminal multiplayer death match game (alpha).
- [LanChat](https://github.com/sigmaSd/LanChat) LanChat flutter + rust demo.

*Does your awesome project use `message-io`? Make a Pull Request and add it to the list!*

## Is message-io for me?
`message-io` has the main goal to keep things simple.
This is great, but sometimes this point of view could make more complex the already complex things.

For instance, `message-io` allows handling asynchronous network events without using an `async/await` pattern.
It reduces the complexity to handle income messages from the network, which is great.
Nevertheless, the applications that read asynchronous messages tend to perform
asynchronous tasks over these events too.
This asynchronous inheritance can easily be propagated to your entire application
being difficult to maintain or scale without an async/await pattern.
In those cases, maybe [`tokio`](https://tokio.rs) could be a better option.
You need to deal with more low-level network stuff but you gain in organization and thread/resource management.

A similar issue can happen regarding the node usage of `message-io`.
Because a node can be used independently as a client/server or both,
you can easily start to make peer to peer applications.
In fact, this is one of the intentions of `message-io`.
Nevertheless, if your goal scales, will appear problems related to this patter to deal with,
and libraries such as [`libp2p`](https://libp2p.io) come with a huge battery of tools to help to archive that goal.

Of course, this is not a disclaiming about the library usage (I use it!),
it is more about being honest about its capabilities,
and to guide you to the right tool depending on what are you looking for.

To summarize:

- If you have a medium complex network problem: make it simpler with `message-io`!
- If you have a really complex network problem: use
  [`tokio`](https://tokio.rs), [`libp2p`](https://libp2p.io) or others, to have more control over it.
