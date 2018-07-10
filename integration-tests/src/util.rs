extern crate config;
extern crate cwrap;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate quick_error;
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use ::std::{env, thread, str};
use ::std::time::Duration;
use ::std::sync::RwLock;
use ::std::error::Error;
use ::std::convert::From;
use ::jedi::{Value, JSONError};

pub use ::cwrap::{send, recv, recv_event, recv_event_nb};

// -----------------------------------------------------------------------------
// Error object
// -----------------------------------------------------------------------------
quick_error! {
    #[derive(Debug)]
    /// Turtl's main error object.
    pub enum TError {
        Msg(err: String) {
            description("string err")
            display("{}", err)
        }
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("{:?}", err)
        }
        JSON(err: JSONError) {
            cause(err)
            description("JSON error")
            display("{:?}", err)
        }
    }
}

pub type TResult<T> = Result<T, TError>;

/// A macro to make it easy to create From impls for TError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for TError {
            fn from(err: $t) -> TError {
                TError::Boxed(Box::new(err))
            }
        }
    )
}

impl From<JSONError> for TError {
    fn from(err: JSONError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            match err {
                JSONError::Boxed(x) => TError::Boxed(x),
                _ => TError::JSON(err),
            }
        }
    }
}
from_err!(::std::string::FromUtf8Error);

// -----------------------------------------------------------------------------
// General test functions
// -----------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    e: u32,
    d: Value,
}

lazy_static! {
    /// create a static/global CONFIG var, and load it with our config data
    static ref MID: RwLock<u64> = RwLock::new(0);
}

#[allow(dead_code)]
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

pub fn init() -> thread::JoinHandle<()> {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }
    // load the local config
    config::load_config(None).unwrap();
    let handle = cwrap::init(r#"{
        "data_folder": ":memory:",
        "wrap_errors": true,
        "messaging": {"reqres_append_mid": true},
        "logging": {"file": null},
        "sync": {
            "enable_incoming": true,
            "enable_outgoing": true,
            "enable_files_incoming": true,
            "enable_files_outgoing": true,
            "poll_timeout": 5
        }
    }"#);
    wait_on("messaging:ready");
    handle
}

pub fn end(handle: thread::JoinHandle<()>) {
    dispatch(json!(["app:shutdown"]));
    handle.join().unwrap();
}

pub fn dispatch(args: Value) -> Response {
    let msg_id = {
        let mut mid_guard = MID.write().unwrap();
        let mid = *mid_guard;
        *mid_guard += 1;
        mid.to_string()
    };
    let mut msg_args = vec![jedi::to_val(&msg_id).unwrap()];
    let mut vals = jedi::from_val::<Vec<Value>>(args).unwrap();
    msg_args.append(&mut vals);
    let msg = jedi::stringify(&msg_args).unwrap();
    send(msg.as_str());
    let recv = recv(msg_id.as_str());
    jedi::parse(&recv).unwrap()
}

pub fn dispatch_ass(args: Value) -> Value {
    let res = dispatch(args);
    if res.e != 0 {
        panic!("dispatch: {}", res.d);
    }
    let Response {e: _e, d} = res;
    d
}

pub fn wait_on(evname: &str) -> Value {
    loop {
        let ev = recv_event();
        let parsed: Value = jedi::parse(&ev).unwrap();
        let parsed_evname: String = jedi::get(&["e"], &parsed).unwrap();
        if parsed_evname == evname {
            return jedi::get(&["d"], &parsed).unwrap();
        }
    }
}

pub fn drain_events() {
    loop {
        let ev = recv_event_nb();
        if ev.is_none() { return; }
    }
}
