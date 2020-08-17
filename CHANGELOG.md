# Changelog

## To be released
- Internal behavior changed: non-blocking TCP stream.
- Internal thread names for debugging easily.
- Fixed some UDP issues.
- Modified *distributed* example to use UDP instead of TCP in the participant connections.

## Current 0.3.1
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
