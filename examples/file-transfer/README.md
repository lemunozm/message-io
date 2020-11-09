# TCP client and server example
This example allows to send and receive files through TCP.

## Test it!
First, choose a file to send it. If it's big, better!
If you are in linux, you can create a 1GB file with the following command:
`truncate -s 1G <filename>`

Launch the receiver in a terminal.
It acts as a server, being able to receive files from several clients at the same time.
```
cargo run --example file-transfer recv
```

Run a sender with a file path (one sender per terminal):
```
cargo run --example file-transfer send <file_path>
```

Note: You can play the with disconnections using `ctrl-c` over the sender/receiver.

## Desing notes
The file is sent in chunks to not block the `EventQueue` if the transfer is really long.

