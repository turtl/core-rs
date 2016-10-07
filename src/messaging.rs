//! The messenger is responsible for proxing messages between our remote and 
//! our main thread.
//!
//! This module is essentially the window into the app, essentially acting as an
//! event bus to/from our remote sender (generally, this is a UI of some sort).

use ::std::thread::{self, JoinHandle};
use ::std::sync::Arc;

use ::crossbeam::sync::MsQueue;
use ::carrier;

use ::config;
use ::error::{TResult, TError};
use ::util;
use ::util::thredder::Pipeline;
use ::dispatch;
use ::turtl::TurtlWrap;
use ::stop;

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
    fn new(channel: String) -> Messenger {
        Messenger {
            bound: true,
            channel_in: format!("{}-core-in", channel),
            channel_out: format!("{}-core-out", channel),
        }
    }

    /// Create a new messenger with channel-in/channel-out flipped
    fn new_reversed(channel: String) -> Messenger {
        let mut messenger = Messenger::new(channel);
        let channtmp = messenger.channel_in;
        messenger.channel_in = messenger.channel_out;
        messenger.channel_out = channtmp;
        messenger
    }

    #[allow(dead_code)]
    /// Blocking receive
    fn recv(&mut self) -> TResult<String> {
        let bytes = try!(carrier::recv(&self.channel_in[..]));
        debug!("messaging: recv: {}", bytes.len());
        String::from_utf8(bytes).map_err(|e| From::from(e))
    }

    /// Non-blocking receive
    fn recv_nb(&mut self) -> TResult<String> {
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
    pub fn send(&mut self, msg: String) -> TResult<()> {
        debug!("messaging: send: {}", msg.len());
        carrier::send_string(&self.channel_out[..], &msg)
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

/// A handy type alias for our messaging system channel.
//pub type MsgThunk = Thunk<&mut Messenger>;
pub type MsgSender = Arc<MsQueue<Box<MsgThunk>>>;

/// Start a thread that handles proxying messages between main and remote.
///
/// Currently, the implementation relies on polling.
pub fn start(tx_main: Pipeline) -> (MsgSender, JoinHandle<()>) {
    // create our main <--> messaging communication channel
    let queue = Arc::new(MsQueue::new());

    // clone a receiver for our queue, since the main ref will be returned
    // from this function
    let recv = queue.clone();

    let handle = thread::spawn(move || {
        // read our bind address from config, otherwise use a default
        let address: String = match config::get(&["messaging", "address"]) {
            Ok(x) => x,
            Err(e) => {
                error!("messaging: problem grabbing address from config, using default: {}", e);
                String::from("inproc://turtl")
            }
        };

        // create our messenger!
        let mut messenger = Messenger::new(address);

        // a simple internal function called when we have a problem and don't
        // want to panic. this stops the messenger and also kills the main
        // thread
        let term_tx_main = tx_main.clone();
        let terminate = move |mut messenger: Messenger| {
            messenger.shutdown();
            stop(term_tx_main.clone());
        };

        // use these to adjust our active/passive poll delay
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
        let mut nn_errcount: u64 = 0;

        while messenger.is_bound() {
            // grab a message from main (non-blocking)
            match recv.try_pop() {
                Some(x) => {
                    let x: Box<MsgThunk> = x;
                    delay = delay_min;
                    counter = 0;
                    x.call_box(&mut messenger);
                },
                None => (),
            }

            // grab a message from our remote (non-blocking)
            match messenger.recv_nb() {
                Ok(x) => {
                    nn_errcount = 0;
                    delay = delay_min;
                    counter = 0;
                    tx_main.push(Box::new(move |turtl: TurtlWrap| {
                        let msg = x;
                        match dispatch::process(turtl, &msg) {
                            Ok(..) => (),
                            Err(e) => error!("messaging: dispatch: {}", e),
                        }
                    }));
                },
                Err(TError::TryAgain) => (),
                Err(e) => {
                    nn_errcount += 1;
                    error!("messaging: problem polling remote socket: {:?}", e);
                    if nn_errcount > 10 {
                        error!("messaging: too many remove failures, leaving");
                        terminate(messenger);
                        break;
                    }
                }
            }

            // sleep our poller. since both our readers are non-blocking, we
            // need a sleep to keep from spinning cpu
            util::sleep(delay);
            // if we have no action on either recv for 20 loops, switch back to
            // long polling (saves cpu)
            counter += 1;
            if counter > 20 { delay = delay_max; }
        }
        info!("messaging::start() -- shutting down");
    });
    (queue, handle)
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
            let mut messenger = Messenger::new(String::from("inproc://turtltest"));
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

        let mut messenger = Messenger::new_reversed(String::from("inproc://turtltest"));
        messenger.send(String::from("ping")).unwrap();
        let response = messenger.recv().unwrap();
        assert_eq!(response, r#"pong"#);
        assert_eq!(grab_locked_bool(&pong), true);
        assert_eq!(grab_locked_bool(&panic), false);
        handle.join().unwrap();
    }
}

