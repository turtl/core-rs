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
    let handle = turtl_core::start(String::from(r#"{"loglevel":"debug","messaging":{"reqres_append_mid":false}}"#));
    let server = Server::bind("127.0.0.1:7472").unwrap();
    println!("- sock server bound, listening");
    for connection in server.filter_map(Result::ok) {
        thread::spawn(move || {
            println!("- new connection!");
            let mut client = connection.accept().unwrap();
            client.set_nonblocking(true).unwrap();
            loop {
                let msg_res = client.recv_message();
                match msg_res {
                    Ok(msg) => {
                        println!("- got msg from ui");
                        match msg {
                            OwnedMessage::Close(_) => { break; }
                            OwnedMessage::Binary(x) => {
                                let msg_str = String::from_utf8(x).unwrap();
                                turtl_core::send(msg_str).unwrap();
                            }
                            OwnedMessage::Text(x) => { turtl_core::send(x).unwrap(); }
                            _ => {}
                        }
                        println!("- send ui msg to dispatch");
                    }
                    Err(_) => {
                    }
                }

                let msg_turtl = turtl_core::recv_nb(None).unwrap();
                match msg_turtl {
                    Some(x) => {
                        println!("- got core message, sending to ui {}", x.len());
                        client.send_message(&Message::text(x)).unwrap();
                    }
                    None => {}
                }

                let msg_turtl = turtl_core::recv_event_nb().unwrap();
                match msg_turtl {
                    Some(x) => {
                        println!("- got core message, sending to ui {}", x.len());
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

