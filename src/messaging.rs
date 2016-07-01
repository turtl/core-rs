use nanomsg::{Socket, Protocol};

use ::config;
use ::error::{TResult, TError};
use ::util;

use std::io::{Read, Write};
use std::sync::RwLock;

/// MessageState holds our main socket for the app, and whether or not the
/// socket is currently bound (which send() checks for when blasting out
/// messages).
struct MessageState {
    bound: bool,
    socket: Socket
}

lazy_static! {
    /// our global MessageState object. it's local to this module allowing the
    /// rest of the app to transparently call bind()/send() without worrying
    /// about passing sockets/state everywhere that needs to send out events.
    /// note that we wrap in RwLock so we can access the values inside mutably,
    /// but this also gives us thread safety for free, which is nice.
    static ref MSGSTATE: RwLock<MessageState> = RwLock::new(MessageState {
        bound: false,
        socket: Socket::new(Protocol::Pair).unwrap()
    });
}

/// bind our messaging (nanomsg) and loop infinitely, grabbing messages as they
/// come in and process them via the given `dispatch` function.
///
/// we log out errors and keep processing, but also catch a special "Shutdown"
/// error that the dispatch function can pass back to tell us to quit the loop
/// and unbind.
pub fn bind(dispatch: &Fn(&String) -> TResult<()>) -> TResult<()> {
    let mut message = String::new();
    let address = try!(config::get_str(&["messaging", "address"]));
    info!("messaging: binding: address: {}", address);

    // bind our socket and mark it as bound so send() knows it can use it
    let mut endpoint = try_t!(((*MSGSTATE).write().unwrap()).socket.bind(&address));
    ((*MSGSTATE).write().unwrap()).bound = true;

    util::sleep(100);

    loop {
        let result = ((*MSGSTATE).write().unwrap()).socket.read_to_string(&mut message);
        match result {
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
    ((*MSGSTATE).write().unwrap()).bound = false;

    Ok(())
}

/// send a message out on an existing socket
pub fn send_sock(socket: &mut Socket, message: &String) -> TResult<()> {
    debug!("messaging: send");
    let msg_bytes = message.as_bytes();
    try_t!(socket.write_all(msg_bytes));
    Ok(())
}

/// grab our global socket from the state and send out a message on it
pub fn send(message: &String) -> TResult<()> {
    if !(*MSGSTATE.write().unwrap()).bound {
        return Err(TError::Msg("messaging: sending on unbound socket".to_owned()));
    }
    send_sock(&mut((*MSGSTATE).write().unwrap()).socket, message)
}

/*
 * functions i want to keep around for testing.
 * 
#[allow(dead_code)]
pub fn send_new(message: &String) -> TResult<()> {
    let mut socket = try_t!(Socket::new(Protocol::Pair));
    let address = try!(config::get_str(&["messaging", "address"]));
    let mut endpoint = try_t!(socket.connect(&address));

    try!(send_sock(&mut socket, &message));
    try_t!(endpoint.shutdown());
    Ok(())
}

#[allow(dead_code)]
pub fn recv(socket: &mut Socket, message: &mut String) -> TResult<()> {
    let address = try!(config::get_str(&["messaging", "address"]));
    info!("messaging: recv: address: {}", address);

    try_t!(socket.read_to_string(message));
    Ok(())
}

#[allow(dead_code)]
pub fn send_recv(outgoing: String, incoming: &mut String) -> TResult<()> {
    let mut socket = try_t!(Socket::new(Protocol::Pair));
    let address = try!(config::get_str(&["messaging", "address"]));
    let mut endpoint = try_t!(socket.connect(&address));

    try!(send_sock(&mut socket, &outgoing));
    try!(recv(&mut socket, incoming));
    try_t!(endpoint.shutdown());
    Ok(())
}
*/

