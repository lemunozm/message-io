# Distributed example
This example allows to create a distributed network of participants based in a discovery server.

## Test it!
Launch the discovery server in a terminal:
```
cargo run --example distributed discovery-server
```

Run a participant (one terminal per participant):
```
cargo run --example distributed participant <name-1>
cargo run --example distributed participant <name-2>
...
cargo run --example distributed participant <name-n>
```

Note: You can play closing the participants (ctrl-c) in order to see the removed notifications.

## Network topology
The participant register and unregister itself in the discovery server, that will notify the rest of the participant about the new existance member.
Then, a participant can create a direct and private communication to another participant.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vRbgpkBItmFwNHnwDs93GZh34hFbn7Ko7AOCby-ntw9Ii2a1XNIXFHC-vfQeUElpulYdb5D0A4D_obG/pub?w=588&h=617"/>
</p>

