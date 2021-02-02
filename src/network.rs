pub use crate::resource_id::{ResourceId, ResourceType};
pub use crate::endpoint::{Endpoint};
pub use crate::adapters::udp::MAX_UDP_LEN;

use crate::adapters::{
    tcp::{TcpAdapter, TcpEvent},
    udp::{UdpAdapter},
};
use crate::encoding::{self, DecodingPool};

use serde::{Serialize, Deserialize};

use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;

use std::convert::TryFrom;
use std::convert::Into;
use std::net::{SocketAddr, ToSocketAddrs};
use std::io::{self};

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
    tcp_adapter: TcpAdapter,
    udp_adapter: UdpAdapter,
    output_buffer: Vec<u8>,
}

impl<'a> Network {
    /// Creates a new [Network].
    /// The user must register an event_callback that can be called
    /// each time the network generate and [NetEvent].
    pub fn new<M, C>(user_event_callback: C) -> Network
    where
        M: for<'b> Deserialize<'b> + Send + 'static,
        C: Fn(NetEvent<M>) + Send + Clone + 'static,
    {
        let mut decoding_pool = DecodingPool::new();

        let event_callback = user_event_callback.clone();
        let tcp_adapter = TcpAdapter::init(Transport::Tcp.into(), move |endpoint, event| {
            match event {
                TcpEvent::Connection => {
                    log::trace!("Connected endpoint {}", endpoint);
                    event_callback(NetEvent::AddedEndpoint(endpoint));
                }
                TcpEvent::Data(data) => {
                    log::trace!("Data received from {}, {} bytes", endpoint, data.len());
                    decoding_pool.decode_from(data, endpoint, |decoded_data| {
                        log::trace!(
                            "Message received from {}, {} bytes",
                            endpoint,
                            decoded_data.len()
                        );
                        match bincode::deserialize::<M>(decoded_data) {
                            Ok(message) => event_callback(NetEvent::Message(endpoint, message)),
                            Err(_) => event_callback(NetEvent::DeserializationError(endpoint)),
                        }
                    });
                }
                TcpEvent::Disconnection => {
                    log::trace!("Disconnected endpoint {}", endpoint);
                    decoding_pool.remove_if_exists(endpoint);
                    event_callback(NetEvent::RemovedEndpoint(endpoint));
                }
            };
        });

        let event_callback = user_event_callback.clone();
        let udp_adapter = UdpAdapter::init(Transport::Udp.into(), move |endpoint, data: &[u8]| {
            log::trace!("Message received from {}, {} bytes", endpoint, data.len());
            match bincode::deserialize::<M>(data) {
                Ok(message) => event_callback(NetEvent::Message(endpoint, message)),
                Err(_) => event_callback(NetEvent::DeserializationError(endpoint)),
            }
        });

        Network { tcp_adapter, udp_adapter, output_buffer: Vec::new() }
    }

    /// Creates a connection to the specific address by TCP.
    /// The endpoint, an identified of the new connection, will be returned.
    /// If the connection can not be performed (e.g. the address is not reached) an error is returned.
    pub fn connect<A: ToSocketAddrs>(
        &mut self,
        transport: Transport,
        addr: A,
    ) -> io::Result<Endpoint>
    {
        let addr = addr.to_socket_addrs().unwrap().next().unwrap();
        match transport {
            Transport::Tcp => self.tcp_adapter.connect(addr),
            Transport::Udp => self.udp_adapter.connect(addr),
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
            Transport::Tcp => self.tcp_adapter.listen(addr),
            Transport::Udp => self.udp_adapter.listen(addr),
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
            Transport::Tcp => self.tcp_adapter.remove(resource_id),
            Transport::Udp => self.udp_adapter.remove(resource_id),
        }
    }

    /// Request a local address of a resource.
    /// Returns `None` if the endpoint id does not exists.
    /// Note: UDP endpoints generated by a listen from UDP shared the resource.
    pub fn local_addr(&self, resource_id: ResourceId) -> Option<SocketAddr> {
        match Transport::try_from(resource_id.adapter_id()).unwrap() {
            Transport::Tcp => self.tcp_adapter.local_address(resource_id),
            Transport::Udp => self.udp_adapter.local_address(resource_id),
        }
    }

    /// Serialize and send the message thought the connection represented by the given endpoint.
    /// If the same message should be sent to different endpoints,
    /// use [Network::send_all()] to get a better performance.
    /// The funcion panics if the endpoint do not exists in the [Network].
    /// If the protocol is UDP, the function panics if the message size is higher than [MAX_UDP_LEN].
    /// If the endpoint disconnects during the sending, a RemoveEndpoint is generated.
    pub fn send<M: Serialize>(&mut self, endpoint: Endpoint, message: M) {
        self.output_buffer.clear();
        match Transport::try_from(endpoint.resource_id().adapter_id()).unwrap() {
            Transport::Tcp => {
                self.prepare_output_message(message);
                self.tcp_adapter.send(endpoint, &self.output_buffer)
            }
            Transport::Udp => {
                // There is no need to encode in UDP
                bincode::serialize_into(&mut self.output_buffer, &message).unwrap();
                self.udp_adapter.send(endpoint, &self.output_buffer)
            }
        }
        log::trace!("Message sent to {}", endpoint);
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
    )
    {
        self.output_buffer.clear();
        self.prepare_output_message(message);
        for endpoint in endpoints {
            match Transport::try_from(endpoint.resource_id().adapter_id()).unwrap() {
                Transport::Tcp => self.tcp_adapter.send(*endpoint, &self.output_buffer),
                Transport::Udp => {
                    // It is preferred to avoid the encoding in UDP in the sending
                    // than check for each endpoint which one is UDP in order to encode it.
                    // The waste is minimum since the encoding is performed one time for all packets
                    self.udp_adapter.send(*endpoint, &self.output_buffer[encoding::PADDING..])
                }
            }
            log::trace!("Message sent to {}", endpoint);
        }
    }

    /// Encodes and serilize a message storing the resuting data in the [Network::output_buffer]
    fn prepare_output_message<M: Serialize>(&mut self, message: M) -> () {
        encoding::encode(&mut self.output_buffer, |enconding_slot| {
            bincode::serialize_into(enconding_slot, &message).unwrap();
        });
    }
}
