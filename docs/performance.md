# Performance

`message-io` is focused in get the better performance,
adding the minimal overhead possible over raw OS socket.

For get this, the library offers some features:
- Zero-copy read/write messages: Messages sent and received pass their data referece (`&[u8]`)
through the library, from the user-space to OS socketn vice-versa without copies.
- Full-duplex: From two different threads, you can simultaneously write and read over the same sockets.
- Multiwriter: Over the same [node](basic_concepts.md), you can write simultaneously from any number of sockets.

## Benchmarks

### Throughput benchmarks
The throughput is the amount of data you can transfer by second, measured in bytes per seconds.

<p align="">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRbPf-P6iKnLV8xStrJq5jIiIl7slzRPRMUOf9WbrPgpa5FeBq6N-qSJkx46T41LzppUmVBIemT1pS3/pub?w=697&h=573"/>
</p>

The following results are measured for 1GB of data, in localhost:

|  Transport |  native  | message-io | efficiency |
|:----------:|:--------:|:----------:|:----------:|
|     UDP    | 7.1 GB/s |  5.9 GB/s  |     83%    |
|     TCP    | 6.4 GB/s |  5.4 GB/s  |     84%    |
| Framed TCP | 5.5 GB/s |  5.0 GB/s  |     91%    |
| Web Socket | 590 MB/s |  560 MB/s  |     95%    |

There are two important comparative: vertical, among different transports,
and horizontal between a native usage and its usage by `message-io`.

Take into account that the throughput is measured by localhost.
In localhost the computer makes few more than a data copy,
so the measured is not hidden by a network latency.
This means that the time difference between the native tests and using `message-io`
measures exactly the overhead that `message-io` adds over the native usage.
In this perjudicial conditions for `message-io`, it maintains an efficiency of `80%`-`90%` which in practice, (in a real network environment with a minimal latency) approaches to `100%`.

If you want to test it yourself, see the [throughput](../examples/throughput) example.

### Latency benchmarks
The latency can be measured as the time a byte take to be trasferred.

<p align="">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vQJ9bhjVWzSnNQLg75Uaed5ZYUIRoJ03OqEg_HSS8VxZqMlvUUFG6ki_mgDc_MDFKrUmbKb8S3eGHvJ/pub?w=692&h=301"/>
</p>

The following results are measured for transfering 1 byte in localhost.

If you want to test it yourself, execute `cargo bench`.
