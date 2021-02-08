use crate::endpoint::{Endpoint};
use crate::util::{SendingStatus};

use mio::event::{Source};

use std::net::{SocketAddr};
use std::io::{self};

/// High level trait to represent an adapter for a transport protocol.
/// The adapter is used to pack a [`Controller`] and [`Adapter`].
/// Two traits to describes how an adapter behaves.
pub trait Adapter {
    type Remote: Source + Send + Sync;
    type Listener: Source + Send + Sync;
    type ActionHandler: ActionHandler<Remote = Self::Remote, Listener = Self::Listener>;
    type EventHandler: EventHandler<Remote = Self::Remote, Listener = Self::Listener>;

    /// Creates a [`Controller`] and [`Processor`] that represents the adapter.
    /// The **implementator** must create their [`Controller`] and [`Processor`] here.
    fn split(self) -> (Self::ActionHandler, Self::EventHandler);
}

/// It is in change to perform direct actions from the user.
pub trait ActionHandler: Send {
    type Remote: Source;
    type Listener: Source;

    /// The user performs a connection request to an specific address.
    /// The **implementator** is in change of creating the corresponding instance in order
    /// to manage it later.
    fn connect(&mut self, addr: SocketAddr) -> io::Result<Self::Remote>;

    /// The user performs a listening request from an specific address.
    /// The **implementator** is in change of creating the corresponding instance in order
    /// to manage it later.
    fn listen(&mut self, addr: SocketAddr) -> io::Result<(Self::Listener, SocketAddr)>;

    /// Sends a raw message by the specific endpoint.
    /// The **implementator** is in charge to send the `data` using the instance represented by
    /// `endpoint.resource_id()`.
    fn send(&mut self, resource: &Self::Remote, endpoint: Endpoint, data: &[u8]) -> SendingStatus;

    fn send_by_listener(
        &mut self,
        _resource: &Self::Listener,
        _endpoint: Endpoint,
        _data: &[u8],
    ) -> SendingStatus
    {
        panic!("Error: You are sending a message from a listener resource");
    }

    fn remove_remote(&mut self, _resource: Self::Remote, _addr: SocketAddr) {}
    fn remove_listener(&mut self, _resource: Self::Listener) {}
}

pub enum AcceptionEvent<'a, R> {
    Remote(SocketAddr, R),
    Data(SocketAddr, &'a [u8]),
}

/// It is in change to perform eventual actions comming from the internal network engine.
/// The `event_callback` is the action that will be performed when an [AdapterEvent] is
/// generated for some `Endpoint`.
/// This function will be produce the [create::network::NetEvent].
pub trait EventHandler: Send {
    type Remote: Source;
    type Listener: Source;

    /// Called when a listener received an event.
    /// It means that an endpoint has try to connect and the connection should accept.
    /// The `id` represents the listener that have generated the event.
    /// The **implementator** is in charge of retrive the instance represented by this `id`
    /// to accept that connection.
    fn accept_event(
        &mut self,
        listener: &Self::Listener,
        event_callback: &mut dyn FnMut(AcceptionEvent<'_, Self::Remote>),
    );

    /// Called when a remote endpoint received an event.
    /// It means that the endpoint has available data to read,
    /// or there is some connection related issue, as a disconnection.
    /// The `id` represents the remote entity that has generated the event.
    /// The **implementator** is in charge of retrive the instance represented by this `id`
    /// and process the event.
    fn read_event(
        &mut self,
        remote: &Self::Remote,
        addr: SocketAddr,
        event_callback: &mut dyn FnMut(&[u8]),
    ) -> bool;
}
