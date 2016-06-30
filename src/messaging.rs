use nanomsg::{Socket, Protocol};

use ::config;
use ::error::{TResult, TError};
use ::util;

use std::io::{Read, Write};
use std::sync::RwLock;

/*
lazy_static! {
    static ref SOCKET: RwLock<Socket> = RwLock::new(Socket::new(Protocol::Pair).unwrap());
    static ref BOUND: RwLock<bool> = RwLock::new(false);
}
*/
static mut SOCKET: Socket = Socket::new(Protocol::Pair).unwrap();

/// bind our messaging (nanomsg)
pub fn bind(dispatch: &Fn(&String) -> TResult<()>) -> TResult<()> {
    let mut message = String::new();
    let address = try!(config::get_str(&["messaging", "address"]));
    info!("messaging: binding: address: {}", address);

    // bind our socket and mark it as bound so send() knows it can use it
    let mut endpoint = try_t!((*SOCKET).write().unwrap().bind(&address));
    (*BOUND.write().unwrap()) = true;

    util::sleep(100);

    loop {
        match (*SOCKET).write().unwrap().read_to_string(&mut message) {
            Ok(..) => {
                info!("messaging: recv");
                match dispatch(&message) {
                    Ok(..) => (),
                    Err(e) => match e {
                        TError::Msg(e) => error!("dispatch: error processing message: {}", e),
                        TError::Shutdown => {
                            warn!("dispatch: got shutdown signal, quitting");
                            util::sleep(10);
                            match send(&"{\"e\":\"shutdown\"}".to_owned()) {
                                Ok(..) => (),
                                Err(..) => (),
                            }
                            util::sleep(10);
                            break;
                        }
                    },
                };
            }
            Err(e) => error!("messaging: error reading message: {}", e),
        }
        message.clear()
    }

    // make sure we shut down the socket and mark it as unbound so any further
    // calls to send() will error
    try_t!(endpoint.shutdown());
    (*BOUND.write().unwrap()) = false;

    Ok(())
}

pub fn send_sock(socket: &mut Socket, message: &String) -> TResult<()> {
    debug!("messaging: send");
    let msg_bytes = message.as_bytes();
    try_t!(socket.write_all(msg_bytes));
    Ok(())
}

pub fn send(message: &String) -> TResult<()> {
    if !(*BOUND.read().unwrap()) {
        return Err(TError::Msg("messaging: sending on unbound socket".to_owned()));
    }
    send_sock(&mut(*SOCKET).write().unwrap(), message)
}

pub fn send_new(message: &String) -> TResult<()> {
    let mut socket = try_t!(Socket::new(Protocol::Bus));
    let address = try!(config::get_str(&["messaging", "address"]));
    let mut endpoint = try_t!(socket.connect(&address));

    try!(send_sock(&mut socket, &message));
    try_t!(endpoint.shutdown());
    Ok(())
}

/*
pub fn recv(message: &mut String) -> TResult<()> {
    let mut socket = try_t!(Socket::new(Protocol::Bus));
    let address = try!(config::get_str(&["messaging", "address"]));
    info!("messaging: binding: address: {}", address);
    let mut endpoint = try_t!(socket.connect(&address));

    match socket.read_to_string(message) {
        Ok(..) => (),
        Err(e) => error!("messaging: error reading message: {}", e),
    }
    try_t!(endpoint.shutdown());
    Ok(())
}
*/

