mod util;
mod resource_id;
mod endpoint;
mod encoding;
mod poll;
mod driver;
mod engine;
mod adapters;
mod remote_addr;
mod transport;

/// Module that offers the pattern to follow to create adapters.
/// This module is not part of the public API itself,
/// it must be used from the internals to build new adapters.
pub mod adapter;

/// Main module of `message-io`.
/// It contains all the resources and tools to create and manage connections.
pub mod network;

/// A set of utilities to deal with asynchronous events.
/// This module mainly offers a synchronized event queue.
/// It can be used alone or along with the network module to reach a synchronized way to deal with
/// events comming from the network.
pub mod events;

pub use adapters::udp::MAX_UDP_PAYLOAD_LEN;
pub use adapters::web_socket::MAX_WS_PAYLOAD_LEN;
