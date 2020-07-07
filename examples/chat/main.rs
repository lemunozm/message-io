mod common;
mod client;
mod server;

use mio::net::{TcpListener, TcpStream};

pub fn main() {
    /*
    TcpStream::connect("127.0.0.1:3001".parse().unwrap()).unwrap();
    TcpListener::bind("127.0.0.1:3001".parse().unwrap()).unwrap();
    loop {
    }
    */
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).unwrap_or(&String::new()).as_ref()
    {
        "client" => client::run(),
        "server" => server::run(),
        _ => println!("Usage: client_server [client | server]")
    }
}

