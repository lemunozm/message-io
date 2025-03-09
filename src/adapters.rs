mod template;

#[cfg(feature = "tcp")]
pub mod tcp;
#[cfg(feature = "tcp")]
pub mod framed_tcp;
#[cfg(feature = "udp")]
pub mod udp;
#[cfg(feature = "websocket")]
pub mod ws;
#[cfg(feature = "unixsocket" )]
pub mod unix_socket;
// Add new adapters here
// ...