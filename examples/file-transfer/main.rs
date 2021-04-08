mod common;
mod sender;
mod receiver;

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).unwrap_or(&String::new()).as_ref() {
        "send" => match args.get(2) {
            Some(file_path) => sender::run(file_path.into()),
            None => println!("The sender needs a 'file path'"),
        },
        "recv" => receiver::run(),
        _ => println!("Usage: recv | (send <filepath>)"),
    }
}
