# UDP client and server example
This example allows to create a *star network topology* using a server and several clients by UDP.

## Test it!
Launch the server in a terminal:
```
cargo run --example udp server
```

Run a client with a name (one client per terminal):
```
cargo run --example udp client <name>
```

Note: Since UDP is not connection-oriented, you can run a client first and then, the server.
