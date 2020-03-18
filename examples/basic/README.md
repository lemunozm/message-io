# Basic client server example
This example allows to create a star network topology using a server and several clients.
They can be connected by Tcp or Udp.

## Test it!
Launch the server in a terminal:
```
cargo run --example basic server
```

Run a client (one client per terminal):
```
cargo run --example basic client
```

Note: in order to use UDP instead of TCP (by default TCP) write 'udp' after `client` or `server`.
