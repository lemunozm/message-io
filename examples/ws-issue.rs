use message_io::network::{NetEvent, Transport, RemoteAddr};
use message_io::node::{self, NodeEvent};

fn main() {
    connect("wss://echo.websocket.org".to_string());
    //connect("ws://echo.websocket.org".to_string());
}

fn connect(url: String) {
    let (handler, listener) = node::split::<()>();
    handler.network().connect(Transport::Ws, RemoteAddr::Str(url.clone())).unwrap();
    listener.for_each(move |event| match event {
        NodeEvent::Network(NetEvent::Connected(e, success)) => {
            if success {
                println!("{} successfully connected", url);
                handler.network().send(e, b"foo");
            }
            else {
                println!("{} failed to connect", url);
                handler.stop();
            }
        }
        NodeEvent::Network(NetEvent::Message(_, x)) => {
            println!("{} received message: {}", url, String::from_utf8_lossy(x));
            handler.stop();
        }
        _ => (),
    });
}
