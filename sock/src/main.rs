#[macro_use]
extern crate log;
extern crate turtl_core;
extern crate websocket;

use ::std::thread;
use ::websocket::{Message, OwnedMessage};
use ::websocket::sync::Server;
use ::std::time::Duration;
use ::std::env;

/// Go to sleeeeep
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

pub fn main() {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }
    turtl_core::init().unwrap();
    let handle = turtl_core::start(String::from(r#"{"messaging":{"reqres_append_mid":false}}"#));
    let server = Server::bind("127.0.0.1:7472").unwrap();
    info!("* sock server bound, listening");
    for connection in server.filter_map(Result::ok) {
        thread::spawn(move || {
            info!("* new connection!");
            let mut client = connection.accept().unwrap();
            client.set_nonblocking(true).unwrap();
            loop {
                let msg_res = client.recv_message();
                match msg_res {
                    Ok(msg) => {
                        match msg {
                            OwnedMessage::Close(_) => { break; }
                            OwnedMessage::Binary(x) => {
                                info!("* ui -> core ({})", x.len());
                                let msg_str = String::from_utf8(x).unwrap();
                                turtl_core::send(msg_str).unwrap();
                            }
                            OwnedMessage::Text(x) => {
                                info!("* ui -> core ({})", x.len());
                                turtl_core::send(x).unwrap();
                            }
                            _ => {}
                        }
                    }
                    Err(_) => {
                    }
                }

                let msg_turtl = turtl_core::recv_nb(None).unwrap();
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui ({})", x.len());
                        client.send_message(&Message::text(x)).unwrap();
                    }
                    None => {}
                }

                let msg_turtl = turtl_core::recv_event_nb().unwrap();
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui ({})", x.len());
                        client.send_message(&Message::text(x)).unwrap();
                    }
                    None => {}
                }
                sleep(100);
            }
        });
    }
    handle.join().unwrap();
}

