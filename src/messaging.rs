//! The messenger is responsible for proxying messages between our remote and 
//! our main thread.
//!
//! This module is essentially the window into the app, essentially acting as an
//! event bus to/from our remote sender (generally, this is a UI of some sort).

use ::carrier;
use ::jedi::{self, Value, Serialize};

use ::config;
use ::error::{TResult, TError};

/// Defines a container for sending responses to the client. We could use a hash
/// table, but then the elements might serialize out of order. This allows us to
/// force our "error" key (`e`) first, and put "data" (`d`) second.
///
/// Note that this is more or less a Turtl-enforced RPC system. Each "call" we
/// run has a response of either error (`e = 1`) or success (`e = 0`) and
/// any supporting data (the error that occurred, or the data we requested).
///
/// NOTE: this is mainly used by the `Turtl` object
#[derive(Serialize)]
#[serde(rename = "res")]
pub struct Response {
    /// The message id
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    /// `e > 0` means "error!!!1", `e == 0` means "great success!!"
    pub e: i64,
    /// Any data we want to pass back to the UI
    pub d: Value,
}

impl Response {
    /// Make a new Response object with a blank id
    pub fn new(e: i64, d: Value) -> Response {
        Response { id: None, e: e, d: d }
    }

    /// Make a new Response object
    pub fn new_w_id(id: String, e: i64, d: Value) -> Response {
        Response { id: Some(id), e: e, d: d }
    }
}

/// Defines a container for sending events to the client. See the `Response`
/// object for notes.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename = "ev")]
pub struct Event {
    /// Our event's name
    pub e: String,
    /// Our event's data
    pub d: Value,
}

pub struct Messenger {
    /// Whether we're bound or not. Kind of vestigial
    bound: bool,

    /// The channel we're listening to
    channel_in: String,

    /// The channel we're sending on
    channel_out: String,
}

impl Messenger {
    /// Create a new messenger with a custom (non-config) channel
    pub fn new_with_channel(channel: String) -> Messenger {
        Messenger {
            bound: true,
            channel_in: format!("{}-core-in", channel),
            channel_out: format!("{}-core-out", channel),
        }
    }

    /// Create a new messenger
    pub fn new() -> Messenger {
        // grab our messaging channel name from config
        let channel: String = match config::get(&["messaging", "reqres"]) {
            Ok(x) => x,
            Err(e) => {
                error!("messaging: problem grabbing address (messaging.reqres) from config, using default: {}", e);
                String::from("inproc://turtl")
            }
        };
        Messenger::new_with_channel(channel)
    }

    #[allow(dead_code)]
    /// Create a new messenger with channel-in/channel-out flipped
    pub fn new_reversed(channel: String) -> Messenger {
        let mut messenger = Messenger::new_with_channel(channel);
        let channtmp = messenger.channel_in;
        messenger.channel_in = messenger.channel_out;
        messenger.channel_out = channtmp;
        messenger
    }

    /// Send an event out to our UI thread. Note that this is a static method!
    pub fn event(name: &str, data: Value) -> TResult<()> {
        let channel: String = config::get(&["messaging", "events"])?;
        let event = Event {
            e: String::from(name),
            d: data,
        };
        let msg = jedi::stringify(&event)?;
        debug!("messaging: event: {} ({})", channel, msg.len());
        carrier::send_string(channel.as_str(), msg)
            .map_err(|e| From::from(e))
    }

    /// Blocking receive
    pub fn recv(&self) -> TResult<String> {
        let bytes = carrier::recv(&self.channel_in[..])?;
        debug!("messaging: recv: {} ({})", self.channel_in, bytes.len());
        String::from_utf8(bytes).map_err(|e| From::from(e))
    }

    #[allow(dead_code)]
    /// Non-blocking receive
    pub fn recv_nb(&self) -> TResult<String> {
        let maybe_bytes = carrier::recv_nb(&self.channel_in[..])?;
        match maybe_bytes {
            Some(x) => {
                debug!("messaging: recv: {} ({})", self.channel_in, x.len());
                String::from_utf8(x).map_err(|e| From::from(e))
            },
            None => Err(TError::TryAgain),
        }
    }

    /// Send a message out
    pub fn send(&self, msg: String) -> TResult<()> {
        debug!("messaging: send: {} ({})", self.channel_out, msg.len());
        carrier::send_string(self.channel_out.as_str(), msg)
            .map_err(|e| From::from(e))
    }

    /// Send a message on the out channel, but suffix the channel
    pub fn send_suffix(&self, suffix: String, msg: String) -> TResult<()> {
        debug!("messaging: send_suffix: {}:{} ({})", self.channel_out, suffix, msg.len());
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

/// Start the messaging system. Essentially does a blocking poll on incoming
/// messages, running the given callback for each one, until the thread gets the
/// "ok, quit!" message.
pub fn start<F>(process: F) -> TResult<()>
    where F: Fn(String) + Send + Sync + 'static
{
    // create our messenger!
    let mut messenger = Messenger::new();
    info!("messaging::start() -- main loop");
    ui_event("messaging:ready", &true)?;
    while messenger.is_bound() {
        // grab a message from our remote
        match messenger.recv() {
            Ok(x) => {
                if x == "turtl:internal:msg:shutdown" {
                    messenger.shutdown();
                    continue;
                }
                process(x);
            },
            Err(e) => {
                error!("messaging: problem polling remote socket: {:?}", e);
            }
        }
    }
    info!("messaging::start() -- shutting down");
    Ok(())
}

/// Call any time to send the "quit" message to the messaging system, which
/// breaks out of its eternal loop.
pub fn stop() {
    let messenger = Messenger::new();
    // send out a shutdown signal on the *incoming* channel so the messaging
    // system gets it
    match messenger.send_rev(String::from("turtl:internal:msg:shutdown")) {
        Ok(_) => {},
        Err(e) => error!("messaging::stop() -- error shutting down messaging thread: {}", e),
    }
}

/// Send an event to our own dispatch handler
pub fn ui_event<T: Serialize>(ev: &str, val: &T) -> TResult<()> {
    info!("messaging::ui_event() -- {}", ev);
    Messenger::event(ev, jedi::to_val(val)?)
}

/// Send an event to our own dispatch handler
pub fn app_event<T: Serialize>(ev: &str, val: &T) -> TResult<()> {
    let messenger = Messenger::new();
    let event = Event {
        e: String::from(ev),
        d: jedi::to_val(val)?,
    };
    messenger.send_rev(format!("::ev{}", jedi::stringify(&event)?))
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
        let guard = lock!(clone);
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
            let messenger = Messenger::new_with_channel(String::from("inproc://turtltest"));
            let message = messenger.recv().unwrap();

            let res = match message.as_ref() {
                "ping" => {
                    let mut pong = lock!(pongref);
                    *pong = true;
                    messenger.send(String::from("pong")).unwrap();
                    Ok(())
                },
                _ => TErr!(TError::Msg(format!("bad command: {}", message))),
            };

            match res {
                Ok(_) => (),
                Err(_) => {
                    let mut panic = lock!(panicref);
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

