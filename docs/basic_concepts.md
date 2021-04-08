# Basic concepts
The main `message-io` entity is the **node**.
It is in charge of managing the different connections, performing actions over them and offering
the events comming from the network or by the own node (called signals).

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vQxs3w6bgIL1Qq600Q0IopWiKvvlKdj9KC7rUuF9Der6sN2UtzYrmn81DPEbRNFmBlEkE1qvDGxwc75/pub?w=890&h=617"/>
</p>

It is splitted in two sides:
- `NodeHandler`: the "input" side, that performs actions.
  Theses actions can be network actions, send signals, or stop the `NodeListener`.
  This entity is clonable and sharable among threads, so you can send messages/signals anywhere from
  your program.
- `NodeListener`: the "output" side, that receives events synchronized from one callback.

And each side has two main entities:
- A synchronized queue (`EventSender` and `EventReceiver`) to send and receive signals from
  the own node.
  This is useful to make sending rates, messages based on timeouts, etc.
- The network that is splited in a `NetworkController` to perform actions, and a `NetworkProcessor`
  to receive events from the network.
  The actions could be create any number of connections/listener, remove them or sending messages.
  The connections are managed by the network and identified by an `Endpoint`, that is a few copiable/hashable struct that represent those connections uniquely.
  It can be understood as the remitter/recipient of the message.
