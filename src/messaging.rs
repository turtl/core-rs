use ::std::io::{Read, Write};
use ::std::thread::{self, JoinHandle};
use ::std::sync::mpsc::{self, Sender, TryRecvError};

use ::nanomsg::{Socket, Protocol};
use ::nanomsg::Error as NanoError;
use ::nanomsg::endpoint::Endpoint;

use ::config;
use ::error::{TResult, TError};
use ::util;
use ::util::thredder::Pipeline;
use ::dispatch;
use ::turtl::Turtl;
use ::stop;

pub struct Messenger {
    socket: Socket,
    endpoint: Option<Endpoint>,
}

impl Messenger {
    fn new() -> Messenger {
        Messenger {
            socket: Socket::new(Protocol::Pair).unwrap(),
            endpoint: None,
        }
    }

    fn bind(&mut self, address: &String) -> TResult<()> {
        info!("messaging: bind: address: {}", address);
        self.endpoint = Some(try_t!(self.socket.bind(address)));
        util::sleep(100);
        Ok(())
    }

    fn connect(&mut self, address: &String) -> TResult<()> {
        info!("messaging: connect: address: {}", address);
        self.endpoint = Some(try_t!(self.socket.connect(address)));
        util::sleep(100);
        Ok(())
    }

    fn recv(&mut self) -> TResult<String> {
        let mut message = String::new();
        try_t!(self.socket.read_to_string(&mut message));
        info!("messaging: recv");
        Ok(message)
    }

    fn recv_nb(&mut self) -> TResult<String> {
        let mut bin = Vec::<u8>::new();
        try!(self.socket.nb_read_to_end(&mut bin).map_err(|e| {
            match e {
                NanoError::TryAgain => TError::TryAgain,
                _ => toterr!(e),
            }
        }));

        let msg = try_t!(String::from_utf8(bin));
        info!("messaging: recv");   // no byte count, no identifying info
        Ok(msg)
    }

    pub fn send(&mut self, msg: String) -> TResult<()> {
        debug!("messaging: send");
        let msg_bytes = msg.as_bytes();
        try_t!(self.socket.write_all(msg_bytes));
        Ok(())
    }

    pub fn shutdown(&mut self) {
        if self.endpoint.is_none() { return; }
        self.endpoint.as_mut().map(|mut x| x.shutdown());
        self.endpoint = None;
    }

    pub fn is_bound(&self) -> bool {
        self.endpoint.is_some()
    }
}

impl Drop for Messenger {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Creates a way to call a Box<FnOnce> basically
pub trait MsgThunk: Send + 'static {
    fn call_box(self: Box<Self>, &mut Messenger);
}
impl<F: FnOnce(&mut Messenger) + Send + 'static> MsgThunk for F {
    fn call_box(self: Box<Self>, messenger: &mut Messenger) {
        (*self)(messenger);
    }
}

pub type MsgSender = Sender<Box<MsgThunk>>;

/// Start a thread that handles proxying messages between main and nanomsg.
///
/// Currently, the implementation relies on polling. This may change once the
/// nanomsg-rs lib gets multithread support (https://github.com/thehydroimpulse/nanomsg.rs/issues/70)
/// but until then we do this the stupid way.
pub fn start(tx_main: Pipeline) -> (MsgSender, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<Box<MsgThunk>>();
    let handle = thread::spawn(move || {
        let mut messenger = Messenger::new();
        let address: String = match config::get(&["messaging", "address"]) {
            Ok(x) => x,
            Err(e) => {
                error!("messaging: problem grabbing address from config, using default: {}", e);
                String::from("inproc://turtl")
            }
        };
        match messenger.bind(&address) {
            Ok(..) => (),
            Err(e) => {
                error!("messaging: error binding {}: {}", address, e);
                return;
            }
        }

        fn terminate(mut messenger: Messenger) {
            messenger.shutdown();
            stop();
        }

        let delay_min: u64 = 1;
        let delay_max: u64 = 100;

        // how long we sleep between polls
        let mut delay: u64 = delay_max;
        // how many iterations we've done since our last incoming message
        let mut counter: u64 = 0;
        // our recv/nano error count. these should ideally never be counted up,
        // and are reset if a successful message does come through on that
        // channel, but otherwise keeps us from doing infinite message loops on
        // a broken channel.
        let mut rx_errcount: u64 = 0;
        let mut nn_errcount: u64 = 0;
        while messenger.is_bound() {
            match rx.try_recv() {
                Ok(x) => {
                    rx_errcount = 0;
                    delay = delay_min;
                    counter = 0;
                    x.call_box(&mut messenger);
                },
                Err(TryRecvError::Empty) => (),
                Err(e) => {
                    rx_errcount += 1;
                    error!("messaging: error receving: {:?}", e);
                    util::sleep(1000);
                    if rx_errcount > 10 {
                        error!("messaging: too many recv failures, leaving");
                        terminate(messenger);
                        break;
                    }
                }
            }
            match messenger.recv_nb() {
                Ok(x) => {
                    nn_errcount = 0;
                    delay = delay_min;
                    counter = 0;
                    let send = tx_main.send(Box::new(move |turtl: &mut Turtl| {
                        let msg = x;
                        match dispatch::process(turtl, &msg) {
                            Ok(..) => (),
                            Err(e) => error!("messaging: dispatch: {}", e),
                        }
                    }));
                    match send {
                        Ok(..) => (),
                        Err(e) => error!("messaging: error proxying nanomsg message to main: {}", e),
                    }
                },
                Err(TError::TryAgain) => (),
                Err(e) => {
                    nn_errcount += 1;
                    error!("messaging: problem polling nanomsg socket: {:?}", e);
                    if nn_errcount > 10 {
                        error!("messaging: too many nanomsg failures, leaving");
                        terminate(messenger);
                        break;
                    }
                }
            }
            util::sleep(delay);
            counter += 1;
            if counter > 20 { delay = delay_max; }
        }
    });
    (tx, handle)
}

/*
/// bind our messaging (nanomsg) and loop infinitely, grabbing messages as they
/// come in and process them via the given `dispatch` function.
///
/// we log out errors and keep processing, but also catch a special "Shutdown"
/// error that the dispatch function can pass back to tell us to quit the loop
/// and unbind.
pub fn bind(dispatch: &mut FnMut(&String) -> TResult<()>) -> TResult<()> {
    let mut message = String::new();
    let address: String = try!(config::get(&["messaging", "address"]));
    info!("messaging: binding: address: {}", address);

    // bind our socket and mark it as bound so send() knows it can use it
    let mut endpoint = try_t!(((*MSGSTATE).write().unwrap()).socket.bind(&address));
    ((*MSGSTATE).write().unwrap()).bound = true;

    util::sleep(100);

    loop {
        let mut guard_msg_state = try_t!((*MSGSTATE).write());
        let result = guard_msg_state.socket.read_to_string(&mut message);
        drop(guard_msg_state);      // release the lock ASAP
        match result {
            Ok(..) => {
                info!("messaging: recv");
                match dispatch(&message) {
                    Ok(..) => (),
                    Err(e) => match e {
                        TError::Msg(e) | TError::BadValue(e) | TError::MissingField(e) | TError::MissingData(e)
                            => error!("dispatch: error processing message: {}", e),
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
            },
            Err(e) => error!("messaging: error reading message: {}", e),
        }
        message.clear()
    }

    // make sure we shut down the socket and mark it as unbound so any further
    // calls to send() will error
    try_t!(endpoint.shutdown());
    let mut guard_msg_state = try_t!((*MSGSTATE).write());
    guard_msg_state.bound = false;
    drop(guard_msg_state);      // release the lock ASAP

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
    send_sock(&mut ((*MSGSTATE).write().unwrap()).socket, message)
}
*/

#[cfg(test)]
mod tests {
    use std::thread;
    use std::sync::{Arc, Mutex};

    use nanomsg::Socket;

    use ::config;
    use super::*;
    use ::error::{TResult, TError};
    use std::io::Read;

    /// receive a message on an open nanomsg socket, saving the message to a
    /// mutable string (passed in)
    fn recv(socket: &mut Socket, message: &mut String) -> TResult<()> {
        let address: String = try!(config::get(&["messaging", "address"]));
        info!("messaging: recv: address: {}", address);

        try_t!(socket.read_to_string(message));
        Ok(())
    }

    /// given a thread-safe bool, return a copy of the bool
    fn grab_locked_bool(val: &Arc<Mutex<bool>>) -> bool {
        let clone = val.clone();
        let guard = clone.lock().unwrap();
        let copy = (*guard).clone();
        copy
    }

    #[test]
    /// spawns a bind() thread, listens for "ping", sets some shared state vars
    /// (to confirm it ran) then shuts down the bind thread.
    ///
    /// this tests that message passing via the messaging system, well, works.
    fn can_bind_send_recv() {
        let pong = Arc::new(Mutex::new(false));
        let panic = Arc::new(Mutex::new(false));

        let panicref = panic.clone();
        let pongref = pong.clone();
        let handle = thread::spawn(move || {
            let mut messenger = Messenger::new();
            messenger.bind(&String::from("inproc://turtltest"));
            let message = messenger.recv().unwrap();

            let res = match message.as_ref() {
                "ping" => {
                    let mut pong = pongref.lock().unwrap();
                    *pong = true;
                    messenger.send(String::from("pong"));
                    Ok(())
                },
                _ => Err(TError::Msg(format!("bad command: {}", message))),
            };

            match res {
                Ok(x) => (),
                Err(e) => {
                    let mut panic = panicref.lock().unwrap();
                    *panic = true;
                }
            }
        });

        let mut messenger = Messenger::new();
        messenger.connect(&String::from("inproc://turtltest"));
        messenger.send(String::from("ping")).unwrap();
        let response = messenger.recv().unwrap();
        assert_eq!(response, r#"pong"#);
        assert_eq!(grab_locked_bool(&pong), true);
        assert_eq!(grab_locked_bool(&panic), false);
    }
}

