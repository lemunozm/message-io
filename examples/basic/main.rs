mod common;
mod client;
mod server;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    let is_tcp = args.get(2).unwrap_or(&"tcp".into()) != "udp";

    match args.get(1).unwrap_or(&String::new()).as_ref()
    {
        "client" => client::run(is_tcp),
        "server" => server::run(is_tcp),
        _ => println!("Usage: basic client | server [tcp(default) | udp]")
    }
}

