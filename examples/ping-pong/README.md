# Ping-Pong example

This example shows a *clients-server* connection.
The clients send a message every second and the server responds to it.

You can run both client and server by several transports: `tcp`, `udp`, `ws` (*WebSocket*).

## Test it!

Launch the server in a terminal:
```
cargo run --example ping-pong server <transport> <port>
```

Run a client (one client per terminal):
```
cargo run --example ping-pong client <transport> (<ip>:<port> | <url>)
```
You can play the disconnections and reconnections using `ctrl-c` over the clients.

*Note: for *WebSocket* (`ws`), you can specify the address as usually `<ip>:<port>` or as an url:
`ws://domain:port/path` for normal websocket or `wss://domain:port/part` for secure web sockets.

