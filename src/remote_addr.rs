use serde::{Serialize, Deserialize};

use std::net::{SocketAddr, ToSocketAddrs, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::io::{self};

/// An struct that contains a remote address.
/// It can be Either, a [`SocketAddr`] as usual or a `String` used for protocols
/// that needs more than a `SocketAddr` to get connected (e.g. WebSocket)
/// It is usually used in [`crate::network::Network::connect()`] to specify the remote address.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
pub enum RemoteAddr {
    SocketAddr(SocketAddr),
    Path(String),
}

impl RemoteAddr {
    /// Check if the `RemoteAddr` is a [`SocketAddr`].
    pub fn is_socket_addr(&self) -> bool {
        matches!(self, RemoteAddr::SocketAddr(_))
    }

    /// Check if the `RemoteAddr` is a path.
    pub fn is_path(&self) -> bool {
        matches!(self, RemoteAddr::SocketAddr(_))
    }

    /// Extract the [`SocketAddr`].
    /// This function panics if the `RemoteAddr` do not represent a `SocketAddr`.
    pub fn socket_addr(&self) -> &SocketAddr {
        match self {
            RemoteAddr::SocketAddr(addr) => addr,
            _ => panic!("The RemoteAddr must be a SocketAddr"),
        }
    }

    /// Extract the path.
    /// This function panics if the `RemoteAddr` do not represent a path.
    pub fn path(&self) -> &str {
        match self {
            RemoteAddr::Path(addr) => addr,
            _ => panic!("The RemoteAddr must be a path"),
        }
    }
}

impl ToSocketAddrs for RemoteAddr {
    type Iter = std::option::IntoIter<SocketAddr>;
    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        match self {
            RemoteAddr::SocketAddr(addr) => addr.to_socket_addrs(),
            RemoteAddr::Path(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The RemoteAddr is not a SocketAddr",
            )),
        }
    }
}

impl std::fmt::Display for RemoteAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteAddr::SocketAddr(addr) => write!(f, "{}", addr),
            RemoteAddr::Path(path) => write!(f, "{}", path),
        }
    }
}

/// Similar to [`ToSocketAddrs`] but for a `RemoteAddr`.
/// Instead of `ToSocketAddrs` that only can accept valid 'ip:port' string format,
/// `ToRemoteAddr` accept any string without panic.
/// If the string has the 'ip:port' format, it will be interpreted as a [`SocketAddr`],
/// if not, it will be interpreted as a string path.
pub trait ToRemoteAddr {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr>;
}

impl ToRemoteAddr for &str {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(match self.parse() {
            Ok(addr) => RemoteAddr::SocketAddr(addr),
            Err(_) => RemoteAddr::Path(self.to_string()),
        })
    }
}

impl ToRemoteAddr for String {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        (&self as &str).to_remote_addr()
    }
}

impl ToRemoteAddr for &String {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        (self as &str).to_remote_addr()
    }
}

impl ToRemoteAddr for SocketAddr {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(*self))
    }
}

impl ToRemoteAddr for SocketAddrV4 {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(SocketAddr::V4(*self)))
    }
}

impl ToRemoteAddr for SocketAddrV6 {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(SocketAddr::V6(*self)))
    }
}

impl ToRemoteAddr for RemoteAddr {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(self.clone())
    }
}

impl ToRemoteAddr for (&str, u16) {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs().unwrap().next().unwrap()))
    }
}

impl ToRemoteAddr for (String, u16) {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs().unwrap().next().unwrap()))
    }
}

impl ToRemoteAddr for (IpAddr, u16) {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs().unwrap().next().unwrap()))
    }
}

impl ToRemoteAddr for (Ipv4Addr, u16) {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs().unwrap().next().unwrap()))
    }
}

impl ToRemoteAddr for (Ipv6Addr, u16) {
    fn to_remote_addr(&self) -> io::Result<RemoteAddr> {
        Ok(RemoteAddr::SocketAddr(self.to_socket_addrs().unwrap().next().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn str_to_path() {
        let path = "ws://domain:1234/socket";
        assert_eq!(path, path.to_remote_addr().unwrap().path());
    }

    #[test]
    fn string_to_path() {
        let path = String::from("ws://domain:1234/socket");
        assert_eq!(&path, path.to_remote_addr().unwrap().path());
    }

    #[test]
    fn str_to_socket_addr() {
        assert!("127.0.0.1:80".to_remote_addr().unwrap().is_socket_addr());
    }

    #[test]
    fn string_to_socket_addr() {
        assert!(String::from("127.0.0.1:80").to_remote_addr().unwrap().is_socket_addr());
    }

    #[test]
    fn socket_addr_to_socket_addr() {
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        assert!(socket_addr.to_remote_addr().unwrap().is_socket_addr());
    }
}
