# Performance

`message-io` is focused in getting the best performance,
adding the minimal possible overhead over raw OS socket.
As performance features, it offers:
- Zero-copy read/write messages: Messages sent and received pass their data referece (`&[u8]`)
through the library, from the user-space to OS socketn vice-versa without copies.
This means that the overhead the library adds is independently of the size of the message,
because the library only copies the poiner to the data, no matter the longer this data is.
- Full-duplex: From two different threads, you can simultaneously write and read over the same socket.
- Multiwriter: Over the same [node](basic_concepts.md), you can write simultaneously from any number of diferent sockets without block for each other.
- One thread: Creating new connections and listeners do not add new threads.
All sockets are managed from one thread, saving resources at scaling.

## Benchmarks

The benchmark compares two dimensions:
- *Vertical*: among different transports. How they behaves among them.
- *Horizontal*: between a *native usage* and its usage by `message-io`.
  A *native usage* is represented here as the **most basic usage** of a transport in a blocking way
  between two sockets, with zero overhead.
  When checking the results, take into account that `message-io` manages for you a pool of sockets that behaves in a non-blocking way waking from a OS poll.
  It will add some delay handling this things.
  However, in most of the network applications you will need to build some kind of socket management on top of the *native usage* by yourself. So, in practice, not using `message-io` would also imply an overhead by implementing a management layer.

*Note that the network benchmark results can vary slightly among different runs,
since it depends considerably on the OS load in the moment of execution.*

### Throughput benchmarks
The throughput is the amount of data you can transfer by second, measured in bytes per seconds.
It starts when the sender sends the first byte and ends when the received receives the last byte.

<p align="">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRbPf-P6iKnLV8xStrJq5jIiIl7slzRPRMUOf9WbrPgpa5FeBq6N-qSJkx46T41LzppUmVBIemT1pS3/pub?w=697&h=573"/>
</p>

The following results are measured for the transmision of 1GB of data by localhost:

|  Transport |  native  | message-io | efficiency |
|:----------:|:--------:|:----------:|:----------:|
|     UDP    | 7.1 GB/s |  5.9 GB/s  |    ~83%    |
|     TCP    | 6.4 GB/s |  5.4 GB/s  |    ~84%    |
| Framed TCP | 5.5 GB/s |  5.0 GB/s  |    ~91%    |
| Web Socket | 590 MB/s |  560 MB/s  |    ~95%    |

Take into account that the throughput is measured by localhost.
In localhost the computer makes few more than a data copy,
so the measured is not hidden by a network latency.
This means that the time difference between the *native usage* and `message-io`
measures exactly the overhead that `message-io` adds over that *native usage*.
In this perjudicial conditions for `message-io`, it maintains an efficiency of `80%`-`90%` which in practice (in a real network environment with a minimal real latency), approaches to `100%`.

If you want to test it yourself, see the [throughput](../examples/throughput) example.

### Latency benchmarks
The latency can be measured as the time a byte take to be trasferred.
It starts when the sender sends a byte and ends when the received receives that byte.

<p align="">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vQJ9bhjVWzSnNQLg75Uaed5ZYUIRoJ03OqEg_HSS8VxZqMlvUUFG6ki_mgDc_MDFKrUmbKb8S3eGHvJ/pub?w=692&h=301"/>
</p>

The following results are measured by transferring 1-byte by localhost:

|  Transport | native | message-io |  increment |
|:----------:|:------:|:----------:|:----------:|
|     UDP    | 1.2 us |   2.1 us   |  + ~0.9 us |
|     TCP    | 2.6 us |   3.5 us   |  + ~0.9 us |
| Framed TCP | 5.2 us |   6.6 us   |  + ~1.4 us |
| Web Socket | 9.1 us |   11.2 us  |  + ~2.1 us |

Depending of the transport used, `message-io` adds arround `1-2us` of overhead per message.
This overhead is contant, because it is zero-copy at reading/writing messages.
It will add it independently of the size of the message, since the library only copies the pointer to the data.
It means, for example, that sending 1B or 64KB of data by *TCP*, `message-io` will add only a `1us` of overhead to the entire data transmision.

If you want to test it yourself, execute `cargo bench`.
