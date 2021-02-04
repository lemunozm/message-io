pub use crate::resource_id::{ResourceId, ResourceType};
pub use crate::endpoint::{Endpoint};
pub use crate::mio_engine::{MioEngine, MioRegister, MioPoll, AdapterEvent};
pub use crate::util::{SendingStatus};

use crate::adapters::{
    tcp::{TcpAdapter, TcpController},
    udp::{UdpAdapter, UdpController},
};

use serde::{Serialize, Deserialize};

use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;

use std::convert::TryFrom;
use std::convert::Into;
use std::net::{SocketAddr, ToSocketAddrs};
use std::io::{self};

/// Enum to identified the underlying transport used.
/// It can be passed in connect/listen functions
#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Transport {
    Tcp,
    Udp,
}

/// Input network events.
#[derive(Debug)]
pub enum NetEvent<M>
where M: for<'b> Deserialize<'b> + Send + 'static
{
    /// Input message received by the network.
    Message(Endpoint, M),

    /// New endpoint added to a listener.
    /// It is sent when a new connection is accepted by the listener.
    /// This event will be sent only in connection oriented protocols as TCP.
    AddedEndpoint(Endpoint),

    /// A connection lost event.
    /// This event is only dispatched when a connection is lost.
    /// Call to [Network::remove_resource()] will NOT generate the event.
    /// After this event, the resource is considered removed.
    /// A Message event will never be generated after this event from the endpoint.
    /// This event will be sent only in connection oriented protocols as TCP.
    /// Because UDP is not connection oriented, the event can no be detected.
    RemovedEndpoint(Endpoint),

    /// This event shows that there was a problem during the deserialization of a message.
    /// The problem is mainly due by a programming issue reading data from an outdated endpoint.
    /// For example: different protocol version.
    /// In production it could be that other application is writing in your application port.
    /// This error means that a message has been lost (the erroneous message),
    /// but the endpoint remains connected for its usage.
    DeserializationError(Endpoint),
}

/// Network is in charge of managing all the connections transparently.
/// It transforms raw data from the network into message events and vice versa,
/// and manages the different adapters for you.
pub struct Network {
    _mio_engine: MioEngine, // Keep it until drop it
    tcp_controller: TcpController,
    udp_controller: UdpController,
    output_buffer: Vec<u8>,              //cached for preformance
    send_all_status: Vec<SendingStatus>, //cached for performance
}

impl Network {
    /// Creates a new [Network].
    /// The user must register an event_callback that can be called
    /// each time the network generate and [NetEvent].
    pub fn new<M, C>(event_callback: C) -> Network
    where
        M: for<'b> Deserialize<'b> + Send + 'static,
        C: Fn(NetEvent<M>) + Send + Clone + 'static,
    {
        /*
        let adapter_event_cb = move |endpoint, event | {
            match event {
                AdapterEvent::Added => {
                    log::trace!("Endpoint connected: {}", endpoint);
                    event_callback(NetEvent::AddedEndpoint(endpoint));
                }
                AdapterEvent::Data(data) => {
                    log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                    match bincode::deserialize::<M>(data) {
                        Ok(message) => event_callback(NetEvent::Message(endpoint, message)),
                        Err(_) => event_callback(NetEvent::DeserializationError(endpoint)),
                    }
                }
                AdapterEvent::Removed => {
                    log::trace!("Endpoint disconnected: {}", endpoint);
                    event_callback(NetEvent::RemovedEndpoint(endpoint));
                }
            }
        };
        */

        let mut mio_poll = MioPoll::new();

        let (tcp_controller, mut tcp_processor)
            = TcpAdapter::split(mio_poll.create_register(Transport::Tcp.into()));

        let (udp_controller, mut udp_processor)
            = UdpAdapter::split(mio_poll.create_register(Transport::Udp.into()));

        let mio_engine = MioEngine::new(mio_poll, move |resource_id| {
            match Transport::try_from(resource_id.adapter_id()).unwrap() {
                Transport::Tcp => tcp_processor.process(resource_id, |endpoint, event| {
                    event_callback(Self::build_net_event(endpoint, event));
                }),
                Transport::Udp => udp_processor.process(resource_id, |endpoint, event| {
                    event_callback(Self::build_net_event(endpoint, event));
                }),
            }
        });

        Network {
            _mio_engine: mio_engine,
            tcp_controller,
            udp_controller,
            output_buffer: Vec::new(),
            send_all_status: Vec::new()
        }
    }

    /// Creates a connection to the specific address by TCP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached)
    /// the corresponding IO error is returned.
    pub fn connect<A: ToSocketAddrs>(
        &mut self,
        transport: Transport,
        addr: A,
    ) -> io::Result<Endpoint>
    {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        match transport {
            Transport::Tcp => self.tcp_controller.connect(addr),
            Transport::Udp => self.udp_controller.connect(addr),
        }
    }

    /// Listen messages from specified transport.
    /// The giver address will be used as interface and listening port.
    /// If the port can be opened, a [ResourceId] identifying the listener is returned
    /// along with the local address, or an error if not.
    /// The address is returned despite you passed as parameter because
    /// when a '0' port is specified, the OS will give a value.
    /// If the protocol is UDP and the address is Ipv4 in the range of multicast ips
    /// (from 224.0.0.0 to 239.255.255.255) it will be listening is multicast mode.
    pub fn listen<A: ToSocketAddrs>(
        &mut self,
        transport: Transport,
        addr: A,
    ) -> io::Result<(ResourceId, SocketAddr)>
    {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        match transport {
            Transport::Tcp => self.tcp_controller.listen(addr),
            Transport::Udp => self.udp_controller.listen(addr),
        }
    }

    /// Remove a network resource.
    /// Returns `None` if the resource id not exists.
    /// This is used mainly to remove resources that the program has been created explicitely,
    /// as connection or listeners.
    /// Resources of endpoints generated by listening from TCP can also be removed
    /// to close the connection.
    /// Note: UDP endpoints generated by listening from UDP shared the resource.
    /// This means that there is no resource to remove as the TCP case.
    /// (in fact there is no connection itself to close, 'there is no spoon').
    pub fn remove_resource(&mut self, resource_id: ResourceId) -> Option<()> {
        match Transport::try_from(resource_id.adapter_id()).unwrap() {
            Transport::Tcp => self.tcp_controller.remove(resource_id),
            Transport::Udp => self.udp_controller.remove(resource_id),
        }
    }

    /// Request a local address of a resource.
    /// Returns `None` if the endpoint id does not exists.
    /// Note: UDP endpoints generated by a listen from UDP shared the resource.
    pub fn local_addr(&self, resource_id: ResourceId) -> Option<SocketAddr> {
        match Transport::try_from(resource_id.adapter_id()).unwrap() {
            Transport::Tcp => self.tcp_controller.local_address(resource_id),
            Transport::Udp => self.udp_controller.local_address(resource_id),
        }
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// If the same message should be sent to different endpoints,
    /// use [Network::send_all()] to get a better performance.
    /// The funcion panics if the endpoint do not exists in the [Network].
    /// If the endpoint disconnects during the sending, a RemoveEndpoint is generated.
    /// A [SendingStatus] is returned with the information about the sending.
    pub fn send<M: Serialize>(&mut self, endpoint: Endpoint, message: M) -> SendingStatus {
        self.output_buffer.clear();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        let status = match Transport::try_from(endpoint.resource_id().adapter_id()).unwrap() {
            Transport::Tcp => self.tcp_controller.send(endpoint, &self.output_buffer),
            Transport::Udp => self.udp_controller.send(endpoint, &self.output_buffer),
        };
        log::trace!("Message sent to {}, {:?}", endpoint, status);
        status
    }

    /// This functions performs the same actions as [Network::send()] but for several endpoints.
    /// When there are severals endpoints to send the data,
    /// this function is faster than consecutive calls to [Network::send()]
    /// since the encoding and serialization is performed only one time for all endpoints.
    /// The funcion panics if some of endpoints do not exists in the [Network].
    pub fn send_all<'b, M: Serialize>(
        &mut self,
        endpoints: impl IntoIterator<Item = &'b Endpoint>,
        message: M,
    ) -> &Vec<SendingStatus>
    {
        self.output_buffer.clear();
        bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
        self.send_all_status.clear();
        for endpoint in endpoints {
            let status = match Transport::try_from(endpoint.resource_id().adapter_id()).unwrap() {
                Transport::Tcp => self.tcp_controller.send(*endpoint, &self.output_buffer),
                Transport::Udp => self.udp_controller.send(*endpoint, &self.output_buffer),
            };
            self.send_all_status.push(status);
            log::trace!("Message sent to {}, {:?}", endpoint, status);
        }
        &self.send_all_status
    }

    fn build_net_event<M>(endpoint: Endpoint, event: AdapterEvent<'_>) -> NetEvent<M>
    where M: for<'b> Deserialize<'b> + Send + 'static {
        match event {
            AdapterEvent::Added => {
                log::trace!("Endpoint connected: {}", endpoint);
                NetEvent::AddedEndpoint(endpoint)
            }
            AdapterEvent::Data(data) => {
                log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                match bincode::deserialize::<M>(data) {
                    Ok(message) => NetEvent::Message(endpoint, message),
                    Err(_) => NetEvent::DeserializationError(endpoint),
                }
            }
            AdapterEvent::Removed => {
                log::trace!("Endpoint disconnected: {}", endpoint);
                NetEvent::RemovedEndpoint(endpoint)
            }
        }
    }
}
