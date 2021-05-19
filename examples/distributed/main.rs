mod common;
mod discovery_server;
mod participant;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).unwrap_or(&String::new()).as_ref() {
        "discovery-server" => match discovery_server::DiscoveryServer::new() {
            Ok(discovery_server) => discovery_server.run(),
            Err(err) => println!("Can not run the discovery server: {}", err),
        },
        "participant" => match args.get(2) {
            Some(name) => match participant::Participant::new(name) {
                Ok(participant) => participant.run(),
                Err(err) => println!("Can not run the participant: {}", err),
            },
            None => println!("The participant needs a 'name'"),
        },
        _ => println!("Usage: discovery-server | participant <name>"),
    }
}
