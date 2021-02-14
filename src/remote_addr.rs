use url::{Url};

use std::net::{SocketAddr, ToSocketAddrs};
use std::io::{self};

/// A struct that contains the a remote address.
/// It is usually used in [`crate::network::Network::connect()`] to specify the server address.
/// It can be Either, an address similar to [`SocketAddr`] or an [`Url`] used for protocols
/// that needs more than the address to get connected (e.g. WebSocket)
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum RemoteAddr {
    SocketAddr(SocketAddr),
    Url(Url),
}

impl RemoteAddr {
    /// Check if the `RemoteAddr` is a [`SocketAddr`].
    pub fn is_socket_addr(&self) -> bool {
        match self {
            RemoteAddr::SocketAddr(_) => true,
            _ => false,
        }
    }

    /// Check if the `RemoteAddr` is an [`Url`].
    pub fn is_url(&self) -> bool {
        match self {
            RemoteAddr::Url(_) => true,
            _ => false,
        }
    }

    /// Trait the `RemoteAddr` as a [`SocketAddr`].
    pub fn socket_addr(&self) -> &SocketAddr {
        match self {
            RemoteAddr::SocketAddr(addr) => addr,
            _ => panic!("The RemoteAddr must be a SocketAddr"),
        }
    }

    /// Trait the `RemoteAddr` as an [`Url`].
    pub fn url(&self) -> &Url {
        match self {
            RemoteAddr::Url(url) => url,
            _ => panic!("The RemoteAddr must be an Url"),
        }
    }
}

pub trait ToRemoteAddr: ToSocketAddrs {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs()?.next().unwrap()))
    }
}

impl ToRemoteAddr for &str {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(match self.parse() {
            Ok(addr) => RemoteAddr::SocketAddr(addr),
            Err(_) => RemoteAddr::Url(
                Url::parse(self)
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Malformed url"))?,
            ),
        })
    }
}

impl ToRemoteAddr for String {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        (&**self).to_remote_addr()
    }
}

impl ToRemoteAddr for SocketAddr {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn str_to_url() {
        assert!("ws://domain:1234/socket".to_remote_addr().unwrap().is_url());
    }

    #[test]
    fn string_to_url() {
        assert!(String::from("ws://domain:1234/socket").to_remote_addr().unwrap().is_url());
    }

    #[test]
    fn string_to_socket_addr() {
        assert!("127.0.0.1:80".to_remote_addr().unwrap().is_socket_addr());
    }

    #[test]
    fn socket_addr_to_socket_addr() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        assert!(socket_addr.to_remote_addr().unwrap().is_socket_addr());
    }
}
