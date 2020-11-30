# Changelog

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
