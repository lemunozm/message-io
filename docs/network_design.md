# Network internal design

The following is how the network module is structured to offer both:
- an API independient of the transport used.
- a way to add a new adapter with the minimal effort.

<p align="center">
  <img src="https://docs.google.com/drawings/d/e/2PACX-1vTIeSO-gXMcOGOCSKwTnKF9Iu7uXBPfhDRDiypZWmOTaqZdP0jTwMkNq_T09xHhxIgSkKlGIKHbwZXB/pub?w=1042&h=629"/>
</p>

There are 4 main layers.
- **network** layer: This is the layer that the user uses to manage `message-io`.
  It can generate actions and also handle the events coming from the internal sockets.
  Also the upper network layer will transform from the *message world* where leaves the user
  into the *raw data world* that `message-io` uses inside, serializing and deserializing
  these message.

- **engine** layer: The engine layer is in charge of managing the actions from the user and
  events weaked from the poll. To do that, he orchestate all the `ActionController` and `EventProcessor` in order to pass the correct action/event to be processed.

- **driver** layer: The `ActionController` and `EventProcessor` live here.
  They are in change of control the each part of the adapter of the transports.
  The first one will control the actions to the transport, and the second one will process the
  events from that adapter. Inside of them, there is the register where the resources will be
  storaged.

- **adapter** layer: This layer consist in two main entities to handle the transport: `ActionHandler`
  and `EventProcessor`. Both entities constitutes the adapter.
