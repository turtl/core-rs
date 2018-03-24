extern crate config;
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

use ::std::{env, thread, slice, str};
use ::std::time::Duration;
use ::std::sync::RwLock;
use ::std::error::Error;
use ::std::convert::From;
use ::std::ffi::CString;
use ::jedi::{Value, JSONError};

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
// Turtl C wrapper
// -----------------------------------------------------------------------------
include!("./bindings.rs");

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

    let handle = thread::spawn(|| {
        // send in a the config options we need for our tests
        let app_config = r#"{
            "data_folder": ":memory:",
            "integration_tests": {"incoming_sync_timeout": 5},
            "wrap_errors": true,
            "messaging": {"reqres_append_mid": true},
            "sync": {
                "enable_incoming": true,
                "enable_outgoing": true,
                "enable_files_incoming": true,
                "enable_files_outgoing": true
            }
        }"#;
        let app_config_c = CString::new(app_config).unwrap();
        let ret = unsafe {
            turtlc_start(app_config_c.as_ptr(), 0)
        };
        if ret != 0 {
            panic!("Error running turtl: err {}", ret);
        }
    });
    wait_on("messaging:ready");
    handle
}

pub fn end(handle: thread::JoinHandle<()>) {
    dispatch(json!(["app:shutdown"]));
    handle.join().unwrap();
}

pub fn send(msg: &str) {
    let msg_vec = Vec::from(String::from(msg).as_bytes());
    let ret = unsafe {
        turtlc_send(msg_vec.as_ptr(), msg_vec.len())
    };
    if ret != 0 {
        panic!("Error sending msg: err {}", ret);
    }
}

pub fn recv(mid: &str) -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let mid_c = CString::new(mid).unwrap();
    let msg_c = unsafe {
        turtlc_recv(0, mid_c.as_ptr(), raw_len)
    };
    assert!(!msg_c.is_null());
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).unwrap();
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
}

pub fn recv_event() -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let msg_c = unsafe {
        turtlc_recv_event(0, raw_len)
    };
    assert!(!msg_c.is_null());
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).unwrap();
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
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

