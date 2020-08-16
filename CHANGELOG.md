## Current 0.3.1
- Fixed issue with the TcpListener.
  Sometimes, a socket was not registered into the connection registry
  if several endpoints connect to a same listener at the same time.
- Added a distributed example based on a discovery server.
- Added network_adapter traces
- Removed Cargo.lock to the tracking files.

## Release 0.3.0
- Added send() method with priority to the EventSender.
- Added Debug trait to NetEvent.
- Minor API changes.
- Fixed hello_world_server example.

## Release 0.2.0
- Fixed issue when a listener is interrupted by OS.
- Added log traces.
- Minor API changes.
- Clean code, rename modules.

## Release 0.1.0
- First release
