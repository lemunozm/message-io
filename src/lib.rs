//! See the [Github README](https://github.com/lemunozm/message-io),
//! to see an overview of this library.

#[cfg(doctest)]
// Tells rustdoc where is the README to compile and test the rust code there
doc_comment::doctest!("../README.md");

mod adapters;

/// Main API.
pub mod node;

/// It contains all the resources and tools to create and manage connections.
pub mod network;

/// A set of utilities to deal with asynchronous events.
/// This module mainly offers a synchronized event queue and timed events.
/// It can be used alone or along with the network module to reach a synchronized way to deal with
/// events comming from the network.
pub mod events;

/// General purpose utilities.
pub mod util;
