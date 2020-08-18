mod common;
mod client;
mod server;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).unwrap_or(&String::new()).as_ref()
    {
        "client" => {
            match args.get(2) {
                Some(name) => client::run(name),
                None => println!("The client needs a 'name'"),
            }
        }
        "server" => server::run(),
        _ => println!("Usage: basic client | server")
    }
}
