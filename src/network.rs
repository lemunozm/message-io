mod resource_id;
mod endpoint;
mod poll;
mod registry;
mod driver;
mod remote_addr;
mod transport;
mod loader;

/// Module that specify the pattern to follow to create adapters.
/// This module is not part of the public API itself,
/// it must be used from the internals to build new adapters.
pub mod adapter;

// Reexports
pub use adapter::{SendStatus};
pub use resource_id::{ResourceId, ResourceType};
pub use endpoint::{Endpoint};
pub use remote_addr::{RemoteAddr, ToRemoteAddr};
pub use transport::{Transport, TransportConnect, TransportListen};
pub use driver::{NetEvent};
pub use poll::{Readiness};

use loader::{DriverLoader, ActionControllerList, EventProcessorList};
use poll::{Poll, PollEvent};

use strum::{IntoEnumIterator};

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};
use std::io::{self};

/// Create a network instance giving its controller and processor.
pub fn split() -> (NetworkController, NetworkProcessor) {
    let mut drivers = DriverLoader::default();
    Transport::iter().for_each(|transport| transport.mount_adapter(&mut drivers));

    let (poll, controllers, processors) = drivers.take();

    let network_controller = NetworkController::new(controllers);
    let network_processor = NetworkProcessor::new(poll, processors);

    (network_controller, network_processor)
}

/// Shareable instance in charge of control all the connections.
pub struct NetworkController {
    controllers: ActionControllerList,
}

impl NetworkController {
    fn new(controllers: ActionControllerList) -> NetworkController {
        Self { controllers }
    }

    /// Creates a connection to the specified address.
    /// The endpoint, an identifier of the new connection, will be returned.
    /// This function will generate a [`NetEvent::Connected`] event with the result of the connection.
    /// This call will **NOT** block to perform the connection.
    ///
    /// Note that this function can return an error in the case the internal socket
    /// could not be binded or open in the OS, but never will return an error an regarding
    /// the connection itself.
    /// If you want to check if the connection has been established or not you have to read the
    /// boolean indicator in the [`NetEvent::Connected`] event.
    ///
    /// Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::{Transport, NetEvent};
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    ///
    /// let (id, addr) = handler.network().listen(Transport::FramedTcp, "127.0.0.1:0").unwrap();
    /// let (conn_endpoint, _) = handler.network().connect(Transport::FramedTcp, addr).unwrap();
    /// // The socket could not be able to send yet.
    ///
    /// listener.for_each(move |event| match event {
    ///     NodeEvent::Network(net_event) => match net_event {
    ///         NetEvent::Connected(endpoint, established) => {
    ///             assert_eq!(conn_endpoint, endpoint);
    ///             if established {
    ///                 println!("Connected!");
    ///                 handler.network().send(endpoint, &[42]);
    ///             }
    ///             else {
    ///                 println!("Could not connect");
    ///             }
    ///         },
    ///         NetEvent::Accepted(endpoint, listening_id) => {
    ///             assert_eq!(id, listening_id);
    ///             println!("New connected endpoint: {}", endpoint.addr());
    ///         },
    ///         _ => (),
    ///     }
    ///     NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// ```
    pub fn connect(
        &self,
        transport: Transport,
        addr: impl ToRemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)> {
        self.connect_with(transport.into(), addr)
    }

    /// Creates a connection to the specified address with custom transport options for transports
    /// that support it.
    /// The endpoint, an identifier of the new connection, will be returned.
    /// This function will generate a [`NetEvent::Connected`] event with the result of the
    /// connection.  This call will **NOT** block to perform the connection.
    ///
    /// Note that this function can return an error in the case the internal socket
    /// could not be binded or open in the OS, but never will return an error regarding
    /// the connection itself.
    /// If you want to check if the connection has been established or not you have to read the
    /// boolean indicator in the [`NetEvent::Connected`] event.
    ///
    /// Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::{TransportConnect, NetEvent};
    /// use message_io::adapters::udp::{UdpConnectConfig};
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    ///
    /// let config = UdpConnectConfig::default().with_broadcast();
    /// let addr = "255.255.255.255:7777";
    /// let (conn_endpoint, _) = handler.network().connect_with(TransportConnect::Udp(config), addr).unwrap();
    /// // The socket could not be able to send yet.
    ///
    /// listener.for_each(move |event| match event {
    ///     NodeEvent::Network(net_event) => match net_event {
    ///         NetEvent::Connected(endpoint, established) => {
    ///             assert_eq!(conn_endpoint, endpoint);
    ///             if established {
    ///                 println!("Connected!");
    ///                 handler.network().send(endpoint, &[42]);
    ///             }
    ///             else {
    ///                 println!("Could not connect");
    ///             }
    ///         },
    ///         _ => (),
    ///     }
    ///     NodeEvent::Signal(_) => handler.stop(),
    /// });
    /// ```
    pub fn connect_with(
        &self,
        transport_connect: TransportConnect,
        addr: impl ToRemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)> {
        let addr = addr.to_remote_addr().unwrap();
        self.controllers[transport_connect.id() as usize].connect_with(transport_connect, addr).map(
            |(endpoint, addr)| {
                log::trace!("Connect to {}", endpoint);
                (endpoint, addr)
            },
        )
    }

    /// Creates a connection to the specified address.
    /// This function is similar to [`NetworkController::connect()`] but will block
    /// until for the connection is ready.
    /// If the connection can not be established, a `ConnectionRefused` error will be returned.
    ///
    /// Note that the `Connect` event will be also generated.
    ///
    /// Since this function blocks the current thread, it must NOT be used inside
    /// the network callback because the internal event could not be processed.
    ///
    /// In order to get the best scalability and performance, use the non-blocking
    /// [`NetworkController::connect()`] version.
    ///
    /// Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::{Transport, NetEvent};
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    ///
    /// let (id, addr) = handler.network().listen(Transport::FramedTcp, "127.0.0.1:0").unwrap();
    /// match handler.network().connect_sync(Transport::FramedTcp, addr) {
    ///     Ok((endpoint, _)) => {
    ///         println!("Connected!");
    ///         handler.network().send(endpoint, &[42]);
    ///     }
    ///     Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
    ///         println!("Could not connect");
    ///     }
    ///     Err(err) => println!("An OS error creating the socket"),
    /// }
    /// ```
    pub fn connect_sync(
        &self,
        transport: Transport,
        addr: impl ToRemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)> {
        self.connect_sync_with(transport.into(), addr)
    }

    /// Creates a connection to the specified address with custom transport options for transports
    /// that support it.
    /// This function is similar to [`NetworkController::connect_with()`] but will block
    /// until for the connection is ready.
    /// If the connection can not be established, a `ConnectionRefused` error will be returned.
    ///
    /// Note that the `Connect` event will be also generated.
    ///
    /// Since this function blocks the current thread, it must NOT be used inside
    /// the network callback because the internal event could not be processed.
    ///
    /// In order to get the best scalability and performance, use the non-blocking
    /// [`NetworkController::connect_with()`] version.
    ///
    /// Example
    /// ```
    /// use message_io::node::{self, NodeEvent};
    /// use message_io::network::{TransportConnect, NetEvent};
    /// use message_io::adapters::udp::{UdpConnectConfig};
    ///
    /// let (handler, listener) = node::split();
    /// handler.signals().send_with_timer((), std::time::Duration::from_secs(1));
    ///
    /// let config = UdpConnectConfig::default().with_broadcast();
    /// let addr = "255.255.255.255:7777";
    /// match handler.network().connect_sync_with(TransportConnect::Udp(config), addr) {
    ///     Ok((endpoint, _)) => {
    ///         println!("Connected!");
    ///         handler.network().send(endpoint, &[42]);
    ///     }
    ///     Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
    ///         println!("Could not connect");
    ///     }
    ///     Err(err) => println!("An OS error creating the socket"),
    /// }
    /// ```
    pub fn connect_sync_with(
        &self,
        transport_connect: TransportConnect,
        addr: impl ToRemoteAddr,
    ) -> io::Result<(Endpoint, SocketAddr)> {
        let (endpoint, addr) = self.connect_with(transport_connect, addr)?;
        loop {
            std::thread::sleep(Duration::from_millis(1));
            match self.is_ready(endpoint.resource_id()) {
                Some(true) => return Ok((endpoint, addr)),
                Some(false) => continue,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        "Connection refused",
                    ))
                }
            }
        }
    }

    /// Listen messages from specified transport.
    /// The given address will be used as interface and listening port.
    /// If the port can be opened, a [ResourceId] identifying the listener is returned
    /// along with the local address, or an error if not.
    /// The address is returned despite you passed as parameter because
    /// when a `0` port is specified, the OS will give choose the value.
    pub fn listen(
        &self,
        transport: Transport,
        addr: impl ToSocketAddrs,
    ) -> io::Result<(ResourceId, SocketAddr)> {
        self.listen_with(transport.into(), addr)
    }

    /// Listen messages from specified transport with custom transport options for transports that
    /// support it.
    /// The given address will be used as interface and listening port.
    /// If the port can be opened, a [ResourceId] identifying the listener is returned
    /// along with the local address, or an error if not.
    /// The address is returned despite you passed as parameter because
    /// when a `0` port is specified, the OS will give choose the value.
    pub fn listen_with(
        &self,
        transport_listen: TransportListen,
        addr: impl ToSocketAddrs,
    ) -> io::Result<(ResourceId, SocketAddr)> {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        self.controllers[transport_listen.id() as usize].listen_with(transport_listen, addr).map(
            |(resource_id, addr)| {
                log::trace!("Listening at {} by {}", addr, resource_id);
                (resource_id, addr)
            },
        )
    }

    /// Send the data message thought the connection represented by the given endpoint.
    /// This function returns a [`SendStatus`] indicating the status of this send.
    /// There is no guarantee that send over a correct connection generates a [`SendStatus::Sent`]
    /// because any time a connection can be disconnected (even while you are sending).
    /// Except cases where you need to be sure that the message has been sent,
    /// you will want to process a [`NetEvent::Disconnected`] to determine if the connection +
    /// is *alive* instead of check if `send()` returned [`SendStatus::ResourceNotFound`].
    pub fn send(&self, endpoint: Endpoint, data: &[u8]) -> SendStatus {
        log::trace!("Sending {} bytes to {}...", data.len(), endpoint);
        let status =
            self.controllers[endpoint.resource_id().adapter_id() as usize].send(endpoint, data);
        log::trace!("Send status: {:?}", status);
        status
    }

    /// Remove a network resource.
    /// Returns `false` if the resource id doesn't exists.
    /// This is used to remove resources as connection or listeners.
    /// Resources of endpoints generated by listening in connection oriented transports
    /// can also be removed to close the connection.
    /// Removing an already connected connection implies a disconnection.
    /// Note that non-oriented connections as UDP use its listener resource to manage all
    /// remote endpoints internally, the remotes have not resource for themselfs.
    /// It means that all generated `Endpoint`s share the `ResourceId` of the listener and
    /// if you remove this resource you are removing the listener of all of them.
    /// For that cases there is no need to remove the resource because non-oriented connections
    /// have not connection itself to close, 'there is no spoon'.
    pub fn remove(&self, resource_id: ResourceId) -> bool {
        log::trace!("Remove {}", resource_id);
        let value = self.controllers[resource_id.adapter_id() as usize].remove(resource_id);
        log::trace!("Removed: {}", value);
        value
    }

    /// Check a resource specified by `resource_id` is ready.
    /// If the status is `true` means that the resource is ready to use.
    /// In connection oriented transports, it implies the resource is connected.
    /// If the status is `false` it means that the resource is not yet ready to use.
    /// If the resource has been removed, disconnected, or does not exists in the network,
    /// a `None` is returned.
    pub fn is_ready(&self, resource_id: ResourceId) -> Option<bool> {
        self.controllers[resource_id.adapter_id() as usize].is_ready(resource_id)
    }
}

/// Instance in charge of process input network events.
/// These events are offered to the user as a [`NetEvent`] its processing data.
pub struct NetworkProcessor {
    poll: Poll,
    processors: EventProcessorList,
}

impl NetworkProcessor {
    fn new(poll: Poll, processors: EventProcessorList) -> Self {
        Self { poll, processors }
    }

    /// Process the next poll event.
    /// This method waits the timeout specified until the poll event is generated.
    /// If `None` is passed as timeout, it will wait indefinitely.
    /// Note that there is no 1-1 relation between an internal poll event and a [`NetEvent`].
    /// You need to assume that process an internal poll event could call 0 or N times to
    /// the callback with diferents `NetEvent`s.
    pub fn process_poll_event(
        &mut self,
        timeout: Option<Duration>,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        let processors = &mut self.processors;
        self.poll.process_event(timeout, |poll_event| {
            match poll_event {
                PollEvent::Network(resource_id, interest) => {
                    let processor = &processors[resource_id.adapter_id() as usize];
                    processor.process(resource_id, interest, &mut |net_event| {
                        log::trace!("Processed {:?}", net_event);
                        event_callback(net_event);
                    });
                }

                #[allow(dead_code)] //TODO: remove it with native event support
                PollEvent::Waker => todo!(),
            }
        });
    }

    /// Process poll events until there is no more events during a `timeout` duration.
    /// This method makes succesive calls to [`NetworkProcessor::process_poll_event()`].
    pub fn process_poll_events_until_timeout(
        &mut self,
        timeout: Duration,
        mut event_callback: impl FnMut(NetEvent<'_>),
    ) {
        loop {
            let now = Instant::now();
            self.process_poll_event(Some(timeout), &mut event_callback);
            if now.elapsed() > timeout {
                break
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};
    use crate::util::thread::{NamespacedThread};

    use test_case::test_case;

    lazy_static::lazy_static! {
        static ref TIMEOUT: Duration = Duration::from_millis(1000);
        static ref LOCALHOST_CONN_TIMEOUT: Duration = Duration::from_millis(5000);
    }

    #[cfg_attr(feature = "tcp", test_case(Transport::Tcp))]
    #[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp))]
    #[cfg_attr(feature = "websocket", test_case(Transport::Ws))]
    fn successful_connection(transport: Transport) {
        let (controller, mut processor) = self::split();
        let (listener_id, addr) = controller.listen(transport, "127.0.0.1:0").unwrap();
        let (endpoint, _) = controller.connect(transport, addr).unwrap();

        let mut was_connected = 0;
        let mut was_accepted = 0;
        processor.process_poll_events_until_timeout(*TIMEOUT, |net_event| match net_event {
            NetEvent::Connected(net_endpoint, status) => {
                assert!(status);
                assert_eq!(endpoint, net_endpoint);
                was_connected += 1;
            }
            NetEvent::Accepted(_, net_listener_id) => {
                assert_eq!(listener_id, net_listener_id);
                was_accepted += 1;
            }
            _ => unreachable!(),
        });
        assert_eq!(was_accepted, 1);
        assert_eq!(was_connected, 1);
    }

    #[cfg_attr(feature = "tcp", test_case(Transport::Tcp))]
    #[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp))]
    #[cfg_attr(feature = "websocket", test_case(Transport::Ws))]
    fn successful_connection_sync(transport: Transport) {
        let (controller, mut processor) = self::split();
        let (_, addr) = controller.listen(transport, "127.0.0.1:0").unwrap();

        let mut thread = NamespacedThread::spawn("test", move || {
            let (endpoint, _) = controller.connect_sync(transport, addr).unwrap();
            assert!(controller.is_ready(endpoint.resource_id()).unwrap());
        });

        processor.process_poll_events_until_timeout(*TIMEOUT, |_| ());

        thread.join();
    }

    #[cfg_attr(feature = "tcp", test_case(Transport::Tcp))]
    #[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp))]
    #[cfg_attr(feature = "websocket", test_case(Transport::Ws))]
    fn unreachable_connection(transport: Transport) {
        let (controller, mut processor) = self::split();

        // Ensure that addr is not using by other process
        // because it takes some secs to be reusable.
        let (listener_id, addr) = controller.listen(transport, "127.0.0.1:0").unwrap();
        controller.remove(listener_id);

        let (endpoint, _) = controller.connect(transport, addr).unwrap();
        assert_eq!(controller.send(endpoint, &[42]), SendStatus::ResourceNotAvailable);
        assert!(!controller.is_ready(endpoint.resource_id()).unwrap());

        let mut was_disconnected = false;
        processor.process_poll_events_until_timeout(*LOCALHOST_CONN_TIMEOUT, |net_event| {
            match net_event {
                NetEvent::Connected(net_endpoint, status) => {
                    assert!(!status);
                    assert_eq!(endpoint, net_endpoint);
                    was_disconnected = true;
                }
                _ => unreachable!(),
            }
        });
        assert!(was_disconnected);
    }

    #[cfg_attr(feature = "tcp", test_case(Transport::Tcp))]
    #[cfg_attr(feature = "tcp", test_case(Transport::FramedTcp))]
    #[cfg_attr(feature = "websocket", test_case(Transport::Ws))]
    fn unreachable_connection_sync(transport: Transport) {
        let (controller, mut processor) = self::split();

        // Ensure that addr is not using by other process
        // because it takes some secs to be reusable.
        let (listener_id, addr) = controller.listen(transport, "127.0.0.1:0").unwrap();
        controller.remove(listener_id);

        let mut thread = NamespacedThread::spawn("test", move || {
            let err = controller.connect_sync(transport, addr).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::ConnectionRefused);
        });

        processor.process_poll_events_until_timeout(*LOCALHOST_CONN_TIMEOUT, |_| ());

        thread.join();
    }

    #[test]
    fn create_remove_listener() {
        let (controller, mut processor) = self::split();
        let (listener_id, _) = controller.listen(Transport::Tcp, "127.0.0.1:0").unwrap();
        assert!(controller.remove(listener_id)); // Do not generate an event
        assert!(!controller.remove(listener_id));

        processor.process_poll_events_until_timeout(*TIMEOUT, |_| unreachable!());
    }

    #[test]
    fn create_remove_listener_with_connection() {
        let (controller, mut processor) = self::split();
        let (listener_id, addr) = controller.listen(Transport::Tcp, "127.0.0.1:0").unwrap();
        controller.connect(Transport::Tcp, addr).unwrap();

        let mut was_accepted = false;
        processor.process_poll_events_until_timeout(*TIMEOUT, |net_event| match net_event {
            NetEvent::Connected(..) => (),
            NetEvent::Accepted(_, _) => {
                assert!(controller.remove(listener_id));
                assert!(!controller.remove(listener_id));
                was_accepted = true;
            }
            _ => unreachable!(),
        });
        assert!(was_accepted);
    }
}
