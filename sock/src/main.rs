extern crate cwrap;
extern crate fern;
#[macro_use]
extern crate log;
extern crate time;
extern crate tungstenite;

mod logger;

use ::std::thread;
use ::std::time::Duration;
use ::std::env;
use ::std::sync::{Arc, RwLock};
use ::std::net::TcpListener;
use ::tungstenite::Message;


/// Go to sleeeeep
fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

fn drain_channels() {
    loop {
        if cwrap::recv_event_nb().is_none() { break; }
    }
    loop {
        if cwrap::recv_nb("").is_none() { break; }
    }
}

pub fn main() {
    logger::setup_logger();

    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }
    let handle = cwrap::init(r#"{"messaging":{"reqres_append_mid":false}}"#);
    let server = TcpListener::bind("127.0.0.1:7472").expect("sock::main() -- failed to bind server");
    info!("* sock server bound, listening");
    let conn_id: Arc<RwLock<u32>> = Arc::new(RwLock::new(0));
    macro_rules! inc_conn_id {
        ($conn:expr) => {
            {
                let mut guard = $conn.write().expect("sock::main() -- failed to grab conn write lock");
                *guard += 1;
                *guard
            }
        }
    }
    macro_rules! get_conn_id {
        ($conn:expr) => {
            {
                let guard = $conn.read().expect("sock::main() -- failed to grab conn read lock");
                *guard
            }
        }
    }
    for stream in server.incoming() {
        let cid = conn_id.clone();
        let this_conn_id = inc_conn_id!(cid);
        thread::spawn(move || {
            info!("* new connection! {}", get_conn_id!(cid));
            let stream = stream.unwrap();
            stream.set_nonblocking(true).expect("sock::main() -- failed to set sock to nonblocking lol");
            let mut client = tungstenite::server::accept(stream).unwrap();
            drain_channels();
            cwrap::send(r#"["0","sync:shutdown",false]"#);
            cwrap::send(r#"["0","user:logout",false]"#);
            cwrap::recv("");
            cwrap::recv("");
            drain_channels();
            client.write_message(Message::text(r#"{"e":"messaging:ready","d":true}"#)).expect("sock::main() -- failed to send ready msg to client");
            loop {
                // make sure that if our stupid lazy connection has been left
                // behind that it is forgotten forever and ever and ever and
                // ever and ever.
                if this_conn_id != get_conn_id!(cid) { break; }

                match client.read_message() {
                    Ok(msg) => {
                        match msg {
                            Message::Close(_) => { break; }
                            Message::Binary(x) => {
                                info!("* ui -> core ({})", x.len());
                                let msg_str = String::from_utf8(x).expect("sock::main() -- do you see what happens, larry? do you see what happens when you pass non-utf8 data, larry? this is what happens, larry.");
                                cwrap::send(msg_str.as_str());
                            }
                            Message::Text(x) => {
                                info!("* ui -> core ({})", x.len());
                                cwrap::send(x.as_str());
                            }
                            _ => {}
                        }
                    }
                    Err(_) => {
                    }
                }

                let msg_turtl = cwrap::recv_nb("");
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui (ev: {})", x.len());
                        //println!("---\n{}", x);
                        client.write_message(Message::text(x)).expect("sock::main() -- failed to send message to stinkin' client");
                    }
                    None => {}
                }

                let msg_turtl = cwrap::recv_event_nb();
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui (res: {})", x.len());
                        //println!("---\n{}", x);
                        client.write_message(Message::text(x)).expect("sock::main() -- failed to send event to stinkin' client");
                    }
                    None => {}
                }
                sleep(10);

            }
        });
    }

    /*
    for connection in server.filter_map(Result::ok) {
        let cid = conn_id.clone();
        let this_conn_id = inc_conn_id!(cid);
        thread::spawn(move || {
            info!("* new connection! {}", get_conn_id!(cid));
            let mut client = connection.accept().expect("sock::main() -- failed to accept connection");
            client.set_nonblocking(true).expect("sock::main() -- failed to set sock to nonblocking lol");
            drain_channels();
            cwrap::send(r#"["0","sync:shutdown",false]"#);
            cwrap::send(r#"["0","user:logout",false]"#);
            cwrap::recv("");
            cwrap::recv("");
            drain_channels();
            client.send_message(&Message::text(r#"{"e":"messaging:ready","d":true}"#)).expect("sock::main() -- failed to send ready msg to client");
            loop {
                // make sure that if our stupid lazy connection has been left
                // behind that it is forgotten forever and ever and ever and
                // ever and ever.
                if this_conn_id != get_conn_id!(cid) { break; }

                let msg_res = client.recv_message();
                match msg_res {
                    Ok(msg) => {
                        match msg {
                            OwnedMessage::Close(_) => { break; }
                            OwnedMessage::Binary(x) => {
                                info!("* ui -> core ({})", x.len());
                                let msg_str = String::from_utf8(x).expect("sock::main() -- do you see what happens, larry? do you see what happens when you pass non-utf8 data, larry? this is what happens, larry.");
                                cwrap::send(msg_str.as_str());
                            }
                            OwnedMessage::Text(x) => {
                                info!("* ui -> core ({})", x.len());
                                cwrap::send(x.as_str());
                            }
                            _ => {}
                        }
                    }
                    Err(_) => {
                    }
                }

                let msg_turtl = cwrap::recv_nb("");
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui (ev: {})", x.len());
                        //println!("---\n{}", x);
                        client.send_message(&Message::text(x)).expect("sock::main() -- failed to send message to stinkin' client");
                    }
                    None => {}
                }

                let msg_turtl = cwrap::recv_event_nb();
                match msg_turtl {
                    Some(x) => {
                        info!("* core -> ui (res: {})", x.len());
                        //println!("---\n{}", x);
                        client.send_message(&Message::text(x)).expect("sock::main() -- failed to send event to stinkin' client");
                    }
                    None => {}
                }
                sleep(10);
            }
            info!("* connection ended! {}", this_conn_id);
        });
    }
    */
    handle.join().expect("sock::main() -- failed to join thread");
}

