mod common;
mod client;
mod server;

use message_io::network_manager::{TransportProtocol};

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    let protocol = match args.get(2).unwrap_or(&String::new()).as_ref() {
        "udp" => TransportProtocol::Udp,
        _ => TransportProtocol::Tcp,
    };

    match args.get(1).unwrap_or(&String::new()).as_ref()
    {
        "client" => client::run(protocol),
        "server" => server::run(protocol),
        _ => println!("Usage: client_server [client | server]")
    }
}

