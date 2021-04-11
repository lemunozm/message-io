mod template;

#[cfg(feature = "tcp")]
pub mod tcp;
#[cfg(feature = "tcp")]
pub mod framed_tcp;
#[cfg(feature = "udp")]
pub mod udp;
#[cfg(feature = "websocket")]
pub mod ws;
// Add new adapters here
// ...
