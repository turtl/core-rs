//! The messenger is responsible for proxing messages between our remote and 
//! our main thread.
//!
//! This module is essentially the window into the app, essentially acting as an
//! event bus to/from our remote sender (generally, this is a UI of some sort).

use ::std::thread::{self, JoinHandle};

use ::carrier;

use ::config;
use ::error::{TResult, TError};
use ::util::thredder::Pipeline;
use ::dispatch;
use ::turtl::TurtlWrap;

pub struct Messenger {
    /// Whether we're bound or not. Kind of vestigial
    bound: bool,

    /// The channel we're listening to
    channel_in: String,

    /// The channel we're sending on
    channel_out: String,
}

impl Messenger {
    /// Create a new messenger
    pub fn new(channel: String) -> Messenger {
        Messenger {
            bound: true,
            channel_in: format!("{}-core-in", channel),
            channel_out: format!("{}-core-out", channel),
        }
    }

    #[allow(dead_code)]
    /// Create a new messenger with channel-in/channel-out flipped
    pub fn new_reversed(channel: String) -> Messenger {
        let mut messenger = Messenger::new(channel);
        let channtmp = messenger.channel_in;
        messenger.channel_in = messenger.channel_out;
        messenger.channel_out = channtmp;
        messenger
    }

    /// Blocking receive
    pub fn recv(&self) -> TResult<String> {
        let bytes = try!(carrier::recv(&self.channel_in[..]));
        debug!("messaging: recv: {}", bytes.len());
        String::from_utf8(bytes).map_err(|e| From::from(e))
    }

    #[allow(dead_code)]
    /// Non-blocking receive
    pub fn recv_nb(&self) -> TResult<String> {
        let maybe_bytes = try!(carrier::recv_nb(&self.channel_in[..]));
        match maybe_bytes {
            Some(x) => {
                debug!("messaging: recv: {}", x.len());
                String::from_utf8(x).map_err(|e| From::from(e))
            },
            None => Err(TError::TryAgain),
        }
    }

    /// Send a message out
    pub fn send(&self, msg: String) -> TResult<()> {
        debug!("messaging: send: {}", msg.len());
        carrier::send_string(&self.channel_out[..], msg)
            .map_err(|e| From::from(e))
    }

    /// Send a message on the out channel, but suffix the channel
    pub fn send_suffix(&self, suffix: String, msg: String) -> TResult<()> {
        debug!("messaging: send_suffix: {}", msg.len());
        carrier::send_string(format!("{}:{}", &self.channel_out, suffix).as_str(), msg)
            .map_err(|e| From::from(e))
    }

    /// Send a message out on the in channel
    pub fn send_rev(&self, msg: String) -> TResult<()> {
        debug!("messaging: send_rev: {}", msg.len());
        carrier::send_string(&self.channel_in[..], msg)
            .map_err(|e| From::from(e))
    }

    /// Shutdown the bound/connected socket endpoint
    pub fn shutdown(&mut self) {
        self.bound = false;
    }

    /// Are we bound/connected?
    pub fn is_bound(&self) -> bool {
        self.bound
    }
}

/// Defines our callback type for the messaging system.
///
/// NOTE!! I'd love to just use util::Thunk<&mut Messenger> here, however it
/// bitches about lifetimes and lifetimes are so horribly infectious that I
/// can't justify rewriting a bunch of shit to satisfy it.
pub trait MsgThunk: Send + 'static {
    fn call_box(self: Box<Self>, &mut Messenger);
}
impl<F: FnOnce(&mut Messenger) + Send + 'static> MsgThunk for F {
    fn call_box(self: Box<Self>, messenger: &mut Messenger) {
        (*self)(messenger);
    }
}

/// Start a thread that handles proxying messages between main and remote.
///
/// Currently, the implementation relies on polling.
pub fn start(tx_main: Pipeline) -> (JoinHandle<()>, Box<Fn() + 'static + Sync + Send>) {
    fn get_channel() -> String {
        // read our bind address from config, otherwise use a default
        match config::get(&["messaging", "address"]) {
            Ok(x) => x,
            Err(e) => {
                error!("messaging: problem grabbing address (messaging.address) from config, using default: {}", e);
                String::from("inproc://turtl")
            }
        }
    }
    let handle = thread::spawn(move || {
        // create our messenger!
        let mut messenger = Messenger::new(get_channel());
        info!("messaging::start() -- main loop");
        while messenger.is_bound() {
            // grab a message from our remote
            match messenger.recv() {
                Ok(x) => {
                    if x == "turtl:internal:msg:shutdown" {
                        messenger.shutdown();
                        continue;
                    }
                    debug!("messaging: recv: {}", x.len());
                    tx_main.push(Box::new(move |turtl: TurtlWrap| {
                        let msg = x;
                        match dispatch::process(turtl, &msg) {
                            Ok(..) => (),
                            Err(e) => error!("messaging: dispatch: {}", e),
                        }
                    }));
                },
                Err(e) => {
                    error!("messaging: problem polling remote socket: {:?}", e);
                }
            }
        }
        info!("messaging::start() -- shutting down");
    });
    let shutdown_fn = || {
        let messenger = Messenger::new(get_channel());
        // send out a shutdown signal on the *incoming* channel so the messaging
        // system gets it
        match messenger.send_rev(String::from("turtl:internal:msg:shutdown")) {
            Ok(_) => (),
            Err(e) => {
                error!("turtl::shutdown() -- error shutting down messaging thread: {}", e)
            }
        }
    };
    (handle, Box::new(shutdown_fn))
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::sync::{Arc, Mutex};

    use super::*;
    use ::error::TError;

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
            let messenger = Messenger::new(String::from("inproc://turtltest"));
            let message = messenger.recv().unwrap();

            let res = match message.as_ref() {
                "ping" => {
                    let mut pong = pongref.lock().unwrap();
                    *pong = true;
                    messenger.send(String::from("pong")).unwrap();
                    Ok(())
                },
                _ => Err(TError::Msg(format!("bad command: {}", message))),
            };

            match res {
                Ok(_) => (),
                Err(_) => {
                    let mut panic = panicref.lock().unwrap();
                    *panic = true;
                }
            }
        });

        let messenger = Messenger::new_reversed(String::from("inproc://turtltest"));
        messenger.send(String::from("ping")).unwrap();
        let response = messenger.recv().unwrap();
        assert_eq!(response, r#"pong"#);
        assert_eq!(grab_locked_bool(&pong), true);
        assert_eq!(grab_locked_bool(&panic), false);
        handle.join().unwrap();
    }
}

