//! See the [Github README](https://github.com/lemunozm/message-io),
//! to see an overview of this library.

#[cfg(doctest)]
// Tells rustdoc where is the README to compile and test the rust code there
doc_comment::doctest!("../README.md");

mod adapters;

/// Module that specify the pattern to follow to create adapters.
/// This module is not part of the public API itself,
/// it must be used from the internals to build new adapters.
pub mod adapter;

/// Main module of `message-io`.
/// It contains all the resources and tools to create and manage connections.
pub mod network;

/// A set of utilities to deal with asynchronous events.
/// This module mainly offers a synchronized event queue and timed events.
/// It can be used alone or along with the network module to reach a synchronized way to deal with
/// events comming from the network.
pub mod events;

/// General purpose utilities.
pub mod util;
