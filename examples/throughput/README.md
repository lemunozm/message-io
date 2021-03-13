# Throughput example

This example shows the throughput among differents transport in both ways: used by `message-io` and used isolated from the library, natively.

The aim is to compare two dimensions: different procols among us and the overhead `message-io` adds.

To run this tests, run:
```sh
cargo run --release --example throughput
```

**NOT FORGET** to run this example with the `--release` flag to get the real measurements.

The throughput is measured by sending *1GB* of data between two connected endpoints by **localhost**.
The measure starts when the sender starts sending the data and finished when the receiver receives
the entire data.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRbPf-P6iKnLV8xStrJq5jIiIl7slzRPRMUOf9WbrPgpa5FeBq6N-qSJkx46T41LzppUmVBIemT1pS3/pub?w=697&h=573"/>
</p>

To know more about `message-io` performance and how to interpret the results,
see the [performance](../../docs/performance_benchmarks.md) document.
