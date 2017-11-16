//! An RPC module that listens on a TCP port and hands the things sent to it
//! directly into our messaging system, and vice versa, listens for outgoing
//! messages and sends them through the TCP pipe.

use ::std::io::{Read, Write};
use ::std::net::TcpListener;
use ::messaging::Messenger;
use ::config;
use ::error::{TError, TResult};
use ::std::thread;
use ::std::sync::{Arc, Mutex};
use ::util;

pub fn run() -> TResult<()> {
    let listener = TcpListener::bind("127.0.0.1:7472")?;
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let messenger = Arc::new(Mutex::new(Messenger::new_reversed(config::get(&["messaging", "reqres"])?)));
                let messenger2 = messenger.clone();
                let sm = Arc::new(Mutex::new(s));
                let sm2 = sm.clone();
                thread::spawn(move || {
                    loop {
                        let msg_recv = {
                            let messenger_guard = lock!(messenger2);
                            if !messenger_guard.is_bound() {
                                break;
                            }
                            messenger_guard.recv_nb()
                        };
                        match msg_recv {
                            Ok(inc) => {
                                let mut sg = lock!(sm2);
                                match sg.write(inc.as_bytes()) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        error!("rpc::run() -- error writing to socket: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(TError::TryAgain) => { }
                            Err(e) => {
                                error!("rpc::run() -- error reading messenger: {}", e);
                            }
                        }
                        util::sleep(1000);
                    }
                });
                loop {
                    let mut out = String::new();
                    {
                        let mut sg = lock!(sm);
                        match sg.read_to_string(&mut out) {
                            Ok(_) => {}
                            Err(e) => {
                                error!("rpc::run() -- error reading socket: {}", e);
                                break;
                            }
                        }
                    }
                    let messenger_guard = lock!(messenger);
                    match messenger_guard.send(out) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("rpc::run() -- error passing message to messenger: {}", e);
                            break;
                        }
                    }
                }
                let mut messenger_guard = lock!(messenger);
                messenger_guard.shutdown();
            }
            Err(e) => {
                error!("rpc::run() -- error accepting incoming: {}", e);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "rpc-test")]
    fn runs() {
        run().unwrap();
    }
}

