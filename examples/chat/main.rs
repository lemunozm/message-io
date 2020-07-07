mod common;
mod client;
mod server;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).unwrap_or(&String::new()).as_ref()
    {
        "client" => client::run(),
        "server" => server::run(),
        _ => println!("Usage: client_server [client | server]")
    }
}

