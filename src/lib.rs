//! See the [Github README](https://github.com/lemunozm/message-io),
//! to see an overview of this library.

mod util;
mod resource_id;
mod endpoint;
mod poll;
mod registry;
mod driver;
mod engine;
mod adapters;
mod remote_addr;
mod transport;
mod network_thread;

/// Main module of `message-io`.
/// It contains all the resources and tools to create and manage connections.
pub mod network;

/// A set of utilities to deal with asynchronous events.
/// This module mainly offers a synchronized event queue and timed events.
/// It can be used alone or along with the network module to reach a synchronized way to deal with
/// events comming from the network.
pub mod events;

/// Module that specify the pattern to follow to create adapters.
/// This module is not part of the public API itself,
/// it must be used from the internals to build new adapters.
pub mod adapter;

/// Frame encoding to convert a data stream into packets.
/// It can be used as a utility to build adapters.
pub mod encoding;
