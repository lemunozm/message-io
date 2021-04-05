//! Check the [Github README](https://github.com/lemunozm/message-io),
//! to see an overview of the library.

#[cfg(doctest)]
// Tells rustdoc where is the README to compile and test the rust code found there
doc_comment::doctest!("../README.md");

mod adapters;

/// Main API.
pub mod node;

/// It contains all the resources and tools to create and manage connections.
pub mod network;

/// A set of utilities to deal with asynchronous events.
/// This module mainly offers a synchronized event queue and timed events.
pub mod events;

/// General purpose utilities.
pub mod util;
