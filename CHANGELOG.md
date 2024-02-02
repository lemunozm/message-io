# Changelog

## Release 0.18.1
- Update tugstenite version.
- Update minor versions of internal dependencies

## Release 0.18.0
- Fix compilation issue in Linux

## Release 0.17.0
- Add `bin_device()` option for TCP: [#158](https://github.com/lemunozm/message-io/pull/158)

## Release 0.16.0
- Use builders for build transport options: [#153](https://github.com/lemunozm/message-io/pull/153)
- New transport options for TCP adapter: source_address and bind_device: [#154](https://github.com/lemunozm/message-io/pull/154)

## Release 0.15.1
- Fixed compilation issue when only `tcp` feature is enabled

## Release 0.15.0
- Configurable network adapters: [#141](https://github.com/lemunozm/message-io/pull/141)
- TCP keepalive configurable option: [#143](https://github.com/lemunozm/message-io/pull/143)
- UDP with configurable source address, reuse address, reuse port: [#145](https://github.com/lemunozm/message-io/pull/145),
  [#146](https://github.com/lemunozm/message-io/pull/146)
- UDP linux mimic behaviour for receive broadcasts [#149](https://github.com/lemunozm/message-io/pull/149)
- Updated dependencies.

## Release 0.14.8
- Fix timer concurrent issue: [#136](https://github.com/lemunozm/message-io/issues/136)

## Release 0.14.7
- Updated mio to 0.8
- Updated cargo clippy

## Release 0.14.6
- Updated dependencies.
- Fixed clippy issues in nightly.

## Release 0.14.5
- Removed a `peer_addr()` call used in a log with potential issues in some cases.

## Release 0.14.4
- Added `EventSender::cancel_timer()` to allow cancel timers already sent.
- Added a `TimerId` as a result of calling `EventSender::send_with_timer()`.

## Release 0.14.3
- `websocket` feature compiling without tls.

## Release 0.14.2
- Fixed isolated compilation with `websocket` feature.
- Fixed multicast issue in Mac.

## Release 0.14.1
- `NodeTask` is now considered *must_use*.

## Release 0.14.0
- Asynchronous connections: `NetworkController::connect()` behaviour modified.
Now it performs a non-blocking connection. Previous behaviour with `connect_sync` version.
- Reduced slightly the websocket latency.
- Adapter API modified to handle easily handshakes.
- Fixed websocket issue that could offer an accepted connection that was yet not valid.
- Added `NetworkController::is_ready()`
- Added `borrow()` method for `StoredNetEvent` to transform in into `NetEvent`.
- Added `is_local()` and `is_remote()` helpers to `ResourceId`.
- Added `SendStatus::ResourceNotAvailable`
- Modified `SendStatus::MaxPacketSizeExceeded`, now it not contains lengths info.
- Renamed `udp::MAX_COMPATIBLE_PAYLOAD_LEN` to `udp::MAX_LOCAL_PAYLOAD_LEN`, value updated
with `udp::MAX_PAYLOAD_LEN`.
- Removed `udp::MAX_PAYLOAD_LEN`.
- Added `udp::MAX_NETWORK_PAYLOAD_LEN`.

#### Update notes
Version 0.14 modifies the [`connect()`](https://docs.rs/message-io/latest/message_io/network/struct.NetworkController.html#method.connect) behaviour to perform a
[non-blocking connections](https://github.com/lemunozm/message-io/issues/79) instead.
It is recommended to use this non-blocking mode in order to get the
best scalability and performance in your application. If you need to perform
a similar blocking connection as before (version 0.13), you can call to [`connect_sync()`](https://docs.rs/message-io/latest/message_io/network/struct.NetworkController.html#method.connect_sync).

Note also that the previous `NetEvent::Connect` has been renamed to `NetEvent::Accepted`.
The current `NetEvent::Connect` behaves as a new event to deal with the new non-blocking connections.
See [`NetEvent`](https://docs.rs/message-io/latest/message_io/network/enum.NetEvent.html) docs for more info.

## Release 0.13.3
- Fixed a bad internal assert.

## Release 0.13.2
- Added `Endpoint::from_listener()` that allows to send datagrams from listeners in non connection-oriented protocols (see the API docs).
- Removed `'static` restriction in the `NodeListener::for_each()` callback.
Now, you can pass references to the callback.
- Faster compilation. Reduced some dependencies.
- Faster `NodeHandler` clone and less memory usage in each instance.

## Release 0.13.1
- Added `NodeListener::enqueue()`.

## Release 0.13.0
- Updated `NodeListener::for_each` to works fully synchronous. `Send` trait of the event callback parameter has been removed to works fine with objects like `Rc` `Cell` or references.
This function no longer returns a `NodeTask`.
- New method `NodeListener::for_each_async` has been added to support the previous behaviour of
`for_each`.
- Added `Debug` to `NodeEvent`.

## Release 0.12.2
- Reduced *WebSocket* latency.

## Release 0.12.1
- *WebSocket* now can returns when send correctly a `SendStatus::MaxPacketSizeExceeded` instead of `ResourceNotFound` if the packet size is exceeded.
- *UDP* has increases the packet size when send.
  Now more bytes per packet can be sent if the OS let it.
- Exported some adapter constants dependent.
- `Transport::max_message_size()` now represents the teorical maximum size (see its related docs).

## Release 0.12.0
- Node concept: `NodeHandler` and `NodeListener`.
- Non-mutable and shared network operations.
- Removed AdapterEvent. Now, the only network event is NetEvent that has been modified to contains a reference to the data instead of an allocated vector.
- Removed `Network` entity that has been substituted by `NetworkController` to handle connect/listen/remove/send and `NetworkProcessor` to handle the input events.
- Remamed `EventQueue`to `EventReceiver`.
- Minor API additions.
- Now UDP never generates disconnection events.
- Increased performance:
  - Latency reduced in arround 66%.
  - Zero-copy message.

#### Update notes
Version 0.12 comes with important API changes (changelog) in order to reach zero-copy write/read goal.
For a soft transition, you can use `NodeListener::enqueue()` (see docs).

## Release 0.11.1
- Reduce the bandwidth of `FramedTcp` transport using variadic encoding instead of constant padding.

## Release 0.11.0
- Implemented `Serialize`/`Deserialize` for `Transport`.
- Implemented `Serialize`/`Deserialize` for `RemoteAddr`.
- Changed `Url` to `String` in `RemoteAddr`.
- Changed `RemoteAddr` variant from `SocketAddr` to `Socket`.
- Changed `RemoteAddr` variant from `Url` to `Str`.
- Increase the `ToRemoteAddr` support for many types.
- Returned value of `Netowork::remove()` as boolean.
  Rationale: Removing a connected connection
  could return `None` in the previous version of `remove` if just a disconnection happen.
  The user probably will use `.unwrap()` on it adding a potential bug.
  Changing from `Option<()>` to `bool` avoid this usage.
- Added `RegisterId` of the listener which generates the connection in `AdapterEvent::Added`
and `NetEvent::Connection` events.

## Release 0.10.2
- Added `try_receive()` to `EventQueue` for non-blocking event read.
- tests and benchmarks running with any number of features enabled.

## Release 0.10.1
- Transport by features: `tcp`, `udp`, `websocket`.

## Release 0.10.0
- Renamed `Transport::Tcp` as `Transport::FramedTcp`.
  **WARNING**: If previously you was using `Transport::Tcp` you probably want to use now
  `Transport::FramedTcp` (that behaves the same).
- Added `Transport::Tcp` that has no encoding layer. Now `Transport::Tcp` is purely TCP.
- Renamed `Transport::max_payload()` to `Transport::max_message_size()`.
- Added `Endpoint` into `AdapterEvent`.
- Fixed `ResourceId` compilation in 32-bits.
- Reverted inner tuple position of `Network::split()` in version 0.8.
  First `Network` then `EventQueue`, as an user would expect.

#### Update notes
If you comming from **0.9.4 o less**, note that `Transport::Tcp` has been renamed
to `Transport::FramedTcp` to be more according to its behaviour.
See more [here](https://docs.rs/message-io/latest/message_io/network/enum.Transport.html).

## Release 0.9.4
- Fixed issue with `ResourceId`.

## Release 0.9.3
- Removed `EventQueue` drop restriction of having already droped the associated senders.

## Release 0.9.2
- Added `Network::split_and_map_from_adapter()`.

## Release 0.9.1
- Fixed an UDP issue that could imply losing packet if they were sent consequently very fast.
- Fixed a TCP encoding issue.
- Fixed a Websocket issue that could imply to lost a message just after being accepted.
- Added burst integration test with order check.

## Release 0.9.0
- Removed serialization. **Rationale:**
  The serialization inside `message-io` was merely a shortcut,
  than in most of the cases reduced the power of the user by save 2-3 lines of their code.
  The serialization should be handle by the user for several reasons:
    - The user could want to decide send the message in base of the serialized data size.
      (e.g. chosing a diffenrent transports if the serialized size exceeds the max packet size)
    - The user could want to perform some action over raw data.
    - The user could make a gateway without needed to deserialize to serialize again.
    - Or simply, there is no need to serialize in some cases.
- `Network::remove_resource()` to `Network::resource()`.
- Exposed `AdapterEvent`.
- Added `Network::split_and_map()`.
- Added Transport::max_payload().
- Exposed `encoding` module.
- Fixed message_size test for big messages.
- Ensured correct drop order in the `EventQueue` and its senders.

## Release 0.8.2
- Non-blocking Websocket acception.
  It will improves the speed when you are listen for a websocket connection.

## Release 0.8.1
- Fixed ping-pong example for `udp` and `ws`.
- Protect Websocket listeners from erronous acceptions.

## Release 0.8.0
- Returned `local_addr` at `connect()` function.
- Fixed ping-pong example.

## Release 0.7.1
- Added WebSocket support based in `tungstenite-rs`
- Added `Network::split()` function.
- Join `udp` and `tcp` examples to make the `ping-pong` example with *WebSocket* support.
- Improved `RemoteAddr` traits.

## Release 0.7.0
- Internal improvements in order to use one thread for all adapters.
- Clean architecture to implement new adapters easier (an internal API).
- Increase the UDP packet size that can be sent.
- Correcly managed ConectionRefused error generated by ICMP in UDP connections.
- Correcty notified when remove a multicast udp connection.
- Fixed some cases when udp multicast was not leaving.
- Fixed some memory leaks at decoding pool.
- Removed unused `Network::local_addr`.
- Renamed from `SendingStatus` to `SendStatus`.
- Renamed from `SendingStatus::RemovedEndpoint` to `SendingStatus::ResourceNotFound`.
- Renamed from `MAX_UDP_LEN` to `MAX_UDP_PAYLOAD_LEN`.
- Renamed from `AddedEndpoint` to `Connected`.
- Renamed from `RemovedEndpoint` to `Disconnected`.
- Added `RemoteAddr` for replace `SocketAddr` in connections.
- Renamed from `Listener` to `Local` in resource contexts.
- Improvement speed in TCP framing.

## Release 0.6.0
- Added concurrent writing and reading from socket/streams in UDP and TCP protocols.
- Removed UDP enconding (improved speed)
- The Resource id from `Endpoint` is now a struct called `ResourceId` instead of `usize`.
- Modified connect/listen API functions.
  Now it make uses of the `Transport` enum in order to specify the transport.
- Removed `listen_udp_multicast()` from network.
  Now it make uses of listen() function.
  If it uses an ipv4 multicast address it is set as multicast.
- Adding a `SendingStatus` to sending functions.

## Release 0.5.1
- Fixed a leak memory with timer events.
- Improved timer events accuracy.
- Fixed decoding issue when the messages length was less than encoding padding.

## Release 0.5.0
- Renamed: `receive_event_timeout` to `receive_timeout`.
- Renaded: `NetworkManager` to `Network`.
- Fixed Reset-by-peer errors.
  Now it generates a RemoveEndpoint event.
- Added DeserializationError Event that is generated when there is a problem at deserializing.
- Removed Result from `send()` and `send_all()` functions.
  Now if some error happens during sending, a RemovedEndpoint will be generated (only in Tcp).

## Release 0.4.6
- Fixed lost decoding memory at disconnection.
- Fixed issue in send methods where sometimes data is lost.
- Fixed lost fragmented UDP.
  Before this change, a huge UDP message could be sent without respect the MTU size, now it panics.
- Fixed rare issue of EventQueue dropping when the sender send and event and the receiver
  is already removed.

## Release 0.4.5
- Fixed enconding issue related several messages in the same data chunk.
- Added `file-transfer` example.
- Added minnor encoding test.

## Release 0.4.4
- Added support for message greater than 2^15.
- Added minnor encoding test.

## Release 0.4.3
- Fixed deconding issue with multiple messages at once.
- Fixed endpoint docs

## Release 0.4.2
- Fixed multicast blocking issue
- Fixed docs

## Release 0.4.1
- Added encoding module.
- Added a encode layer over TCP/UDP.
- Added Basic integration end-to-end tests.
- Added unitary tests for encoding exact messages.

## Release 0.4.0
- Added multicast support.
- Added multicast example.
- Modified `connect()`/`listen()` functions api to make them simpler.
  Before:
    ```rust
    network.connect("127.0.0.1:1234".parse().unwrap(), TransportProtocol::Tcp);
    ```
  Now:
    ```rust
    network.connect_tcp("127.0.0.1:1234");
    ```
- Modified `connect()` returned value, before `Option<(Endpoint, SocketAddr)>`, now `Option<Endpoint>`
  The socket address can extracted using the network instance or keeping the input connection address.
- Modified several `Option` returned values by `Result` in order to get a better management of the errors.
- Modified: the Endpoint is now an structure with two properties, connection id and address.
- Removed address field from AddedEndpoint network event since Endpoint contains it.
- Removed: `endpoint_remote_address` since Endpoint contains the address.
- Removed `TransportProtocol`.
- Free all resources in the *distributed* example.
- Fixed sending by endpoints created by `UdpListener`.
- Fixed bug when sending by UDP in the *basic* example.
- Added a simple *TCP* example.
- Added a simple *UDP* example.
- Removed *basic* example (too many mixed concepts).
  All API is mostly coverad among all examples.
- Fixed some unremoved memory related to UDP.
- Performance improvements.
- Added events unitary tests.

## Release 0.3.2
- Internal behavior changed: non-blocking TCP stream.
- Fixed some UDP issues.
- Internal thread names for debugging easily.
- Modified *distributed* example to use UDP instead of TCP in the participant connections.

## Release 0.3.1
- Fixed issue with the `TcpListener`.
  Sometimes a socket was not registered into the connection registry
  if several endpoints connect to a same listener at the same time.
- Added a distributed example based on a discovery server.
- Added traces to `network_adapter` module
- Removed `Cargo.lock` to the tracking files.

## Release 0.3.0
- Added `send_with_priority()` method to the `EventSender` in order to emulate a LIFO in some cases.
- Added Debug trait to `NetEvent`.
- Minor API changes.
- Fixed *hello_world_server* example.

## Release 0.2.0
- Fixed issue when a listener is interrupted by OS.
- Added log traces.
- Minor API changes.
- Clean code, rename modules.

## Release 0.1.0
- First release
