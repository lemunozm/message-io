# Basic concepts
The library has two main modules:

- **`EventQueue`**:
It is a generic and synchronized queue where all the network events, among others, are sent.
The user must be read these events in order to dispatch actions.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vQr06OL40IWagXWHoyytUIlR1SHoahYE0Pkj6r0HmokaUMW4ojC5MV2OViFO9m-2jDqrDokPJ62oSzg/pub?w=837&h=313"/>
</p>

- **`Network`**:
It is an abstraction layer of the transport protocols that works over *non-blocking* sockets.
It allows to create/remove connections, send and receive messages (defined by the user).

To manage the connections, the `Network` offers an *`Endpoint`*
that is an unique identifier of the connection. It can be used
send, remove, or identify received messages.
It can be understood as the remitter/recipient of the message.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vS3y1BKwPHjoFqtHm2pqfmvxr0JRQIzeRJim9s2UOrOIS74cGwlyqxnH4_DHVXTverziCjPzl6FtQMe/pub?w=586&h=273"/>
</p>

The power comes when both pieces joins together, allowing to process all actions from one thread.
To reach this, the user has to connect the `Network` to the `EventQueue` sending the `NetEvent` produced by the first one.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vT6IuBVr4mLbdNfs2yZayqqUJ04PsuqG27Ce3Vdr0ZG8ItX3slISoKVxyndybaYPIS5oFZ6N4TljrKQ/pub?w=701&h=383"/>
</p>

