[![](https://img.shields.io/crates/v/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/crates/l/message-io)](https://www.apache.org/licenses/LICENSE-2.0.txt)
[![](https://img.shields.io/crates/d/message-io)](https://crates.io/crates/message-io)
[![](https://img.shields.io/github/workflow/status/lemunozm/message-io/message-io%20ci)](https://github.com/lemunozm/message-io/actions?query=workflow%3A%22message-io+ci%22)

# message-io
`message-io` is an asynchronous message library to build network applications **easily** and **fast**.
The library manages and processes the socket data streams in order to offer a simple
event message API to the user.
Working as a **generic network manager**, it allows you to implement your own protocol
following some rules, delegating to the library the tedious asynchrony and thread management.
See more [here](#custom-adapter).

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vSPmycMsWoQq60MPEODcakFQVPkDwVy98AnduTswFNPGBB5dpbIsSCHHBhS2iEuSUtbVaYQb7zgfgjO/pub?w=653&h=305" width="653"/>
</p>

If you find a problem using the library or you have an improvement idea, do not hesitate to open an issue. **Any contribution is welcome!**

## Motivation
Managing sockets is hard because you need to fight with threads, concurrency,
IO errors that come from the OS (which are really difficult to understand in some situations),
serialization, encoding...
And if you make use of non-blocking sockets, it adds a new layer of complexity:
synchronize the events that come asynchronously from the OS poll.

`message-io` offers an easy way to deal with all these mentioned problems,
making them transparently for you,
the programmer that wants to make your application with its own problems.
For that, `message-io` offers a simple API and give only two concepts to understand:
**messages** (the data you send and receive), and **endpoints** (the recipients of that data).
This abstraction also offers the possibility to use the same API independently
of the transport protocol used.
You could change the protocol of your application in literally one line.

## Features
- Asynchronous: internal poll event with non-blocking sockets using [mio](https://github.com/tokio-rs/mio).
- Multiplatform: see [mio platform support](https://github.com/tokio-rs/mio#platforms).
- TCP and UDP (with multicast option) protocols.
- Internal encoding layer: handle messages, not data streams.
- FIFO events with timers and priority.
- Easy, intuitive and consistent API:
  - Follows [KISS principle](https://en.wikipedia.org/wiki/KISS_principle).
  - Abstraction from transport layer: do not think about sockets, think about messages and endpoints.
  - Only two main entities to use:
    - an extensible *event-queue* to manage all events synchronously,
    - a *network* that manage all connections (connect, listen, remove, send, receive).
  - Forget concurrence problems: handle thousands of active connections and listeners without any
    effort, "One thread to rule them all".
  - Easy error handling.
    Do not deal with dark internal `std::io::Error` when send/receive from the network.
- High performance:
    - One thread for manage all internal connections over the faster OS poll.
    - Binary serialization.
    - Simultaneous reading/writing operations over same internal OS sockets.
    - Small runtime overhead over OS sockets.

## Getting started
Add to your `Cargo.toml`
```
message-io = "0.7"
```

### Documentation
- [API documentation](https://docs.rs/message-io/)
- [Basic concepts](docs/basic_concepts.md)
- [Examples](examples):

  - [Basic TCP client and server](examples/tcp)
  - [Basic UDP client and server](examples/udp)
  - [Multicast](examples/multicast)
  - [Distributed network with discovery server](examples/distributed)
  - [File transfer](examples/file-transfer)

- Applications using `message-io`:
  - [Termchat](https://github.com/lemunozm/termchat): Distributed LAN chat in the terminal.
  - [AsciiArena](https://github.com/lemunozm/asciiarena): Terminal multiplayer deathmatch game.
    (under development, but the communication part using `message-io` is almost complete for reference).

### TCP & UDP echo server
The following example is the simplest server that reads messages from the clients and respond to them.
It is capable to manage several client connections and listen from 2 differents protocols at same time.

```rust
use message_io::events::{EventQueue};
use message_io::network::{Network, NetEvent, Transport};

use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
enum InputMessage {
    HelloServer(String),
    // Other input messages here
}

#[derive(Serialize)]
enum OutputMessage {
    HelloClient(String),
    // Other output messages here
}

enum Event {
    Network(NetEvent<InputMessage>),
    // Other user events here
}

fn main() {
    let mut event_queue = EventQueue::new();

    // Create Network, the callback will push the network event into the event queue
    let sender = event_queue.sender().clone();
    let mut network = Network::new(move |net_event| sender.send(Event::Network(net_event)));

    // Listen from TCP and UDP messages on ports 3005.
    let addr = "0.0.0.0:3005";
    network.listen(Transport::Tcp, addr).unwrap();
    network.listen(Transport::Udp, addr).unwrap();

    loop {
        match event_queue.receive() { // Read the next event or wait until have it.
            Event::Network(net_event) => match net_event {
                NetEvent::Message(endpoint, message) => match message {
                    InputMessage::HelloServer(msg) => {
                        println!("Received: {}", msg);
                        network.send(endpoint, OutputMessage::HelloClient(msg));
                    },
                    //Other input messages here
                },
                NetEvent::Connected(_endpoint) => println!("TCP Client connected"),
                NetEvent::Disconnected(_endpoint) => println!("TCP Client disconnected"),
                NetEvent::DeserializationError(_) => (),
            },
            // Other events here
        }
    }
}
```

## Test yourself!
Clone the repository and test the TCP example that you can found in [`examples/tcp`](examples/tcp):

Run the server:
```
cargo run --example tcp server
```
In other terminals, run one or more clients:
```
cargo run --example tcp client <name>
```

## Do you need a transport protocol that `message-io` doesn't have? Add an adapter! <span id="custom-adapter"><span>

`message-io` offers two *kinds* of APIs.
The **user API**, that talks to `message-io` itself as an user that want to use the library,
and the internal **adapter API** for those who want to add their protocol adapters into the library.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRMwZsL8Tki3Sq9Zc2hpZ8L3bJPuj38zgiMKzBCXsX3wrPnfyG2hp-ijmDFUPqicEQZFeyUFxhcdJMB/pub?w=703&h=328"/>
</p>

If the protocol can be built in top on [`mio`](https://github.com/tokio-rs/mio#platforms)
(most of the existing protocol libraries can), then you can add it to `message-io` **really easy**:

1. Add your *adapter* file in `src/adapters/<my-transport-protocol>.rs` that implements the
  traits that you find in [`src/adapter.rs`](src/adapter.rs) (only 6 mandatory functions to implement, see the [template](examples/adapter_template.rs) from examples).

1. Add a new field in the `Transport` enum found in [`src/network.rs`] to register your new adapter.

That's all! You can use your new transport with the `message-io` API like any other.

Oops, one step more, make a *Pull request* for everyone to use it :)

