pub mod template;
#[cfg(feature = "tcp")]
pub mod tcp;
#[cfg(feature = "tcp")]
pub mod framed_tcp;
#[cfg(feature = "udp")]
pub mod udp;
#[cfg(feature = "websocket")]
pub mod web_socket;
// Add new adapters here
// ...
