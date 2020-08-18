# TCP client and server example
This example allows to create a *star network topology* using a server and several clients by TCP.

## Test it!
Launch the server in a terminal:
```
cargo run --example tcp server
```

Run a client with a name (one client per terminal):
```
cargo run --example tcp client <name>
```

Note: You can play the disconnections/reconnections using `ctrl-c` over the clients.
