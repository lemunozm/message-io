mod common;
mod client;
mod server;

use message_io::network::{Transport, ToRemoteAddr};

use std::net::{ToSocketAddrs};

const HELP_MSG: &str = concat!(
    "Usage: ping-pong server (tcp | udp | ws) <port>\n",
    "       pong-pong client (tcp | udp | ws) (<ip>:<port> | url)"
);

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    let transport = match args.get(2).unwrap_or(&"".into()).as_ref() {
        "tcp" => Transport::FramedTcp, // The non-streamed version of tcp.
        "udp" => Transport::Udp,
        "ws" => Transport::Ws,
        _ => return println!("{}", HELP_MSG),
    };

    match args.get(1).unwrap_or(&"".into()).as_ref() {
        "client" => match args.get(3) {
            Some(remote_addr) => {
                let remote_addr = remote_addr.to_remote_addr().unwrap();
                client::run(transport, remote_addr);
            }
            None => return println!("{}", HELP_MSG),
        },
        "server" => {
            match args.get(3).unwrap_or(&"".into()).parse() {
                Ok(port) => {
                    let addr = ("0.0.0.0", port).to_socket_addrs().unwrap().next().unwrap();
                    server::run(transport, addr);
                }
                Err(_) => return println!("{}", HELP_MSG),
            };
        }
        _ => return println!("{}", HELP_MSG),
    }
}
