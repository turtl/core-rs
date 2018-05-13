#![recursion_limit="128"]

extern crate carrier;
extern crate clippo;
extern crate clouseau;
extern crate config;
extern crate crossbeam;
extern crate dumpy;
extern crate fern;
extern crate futures;
extern crate futures_cpupool;
extern crate glob;
extern crate hyper;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
extern crate lib_permissions;
#[macro_use]
extern crate log;
extern crate migrate;
extern crate num_cpus;
#[macro_use]
extern crate protected_derive;
#[macro_use]
extern crate quick_error;
extern crate regex;
extern crate rusqlite;
extern crate rustc_serialize as serialize;  // for hex/base64
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate sodiumoxide;
extern crate time;

#[macro_use]
pub mod error;
#[macro_use]
mod util;
mod crypto;
mod messaging;
mod api;
#[macro_use]
mod sync;
#[macro_use]
mod models;
mod profile;
mod storage;
mod search;
mod dispatch;
mod schema;
mod turtl;

use ::std::thread;
use ::std::sync::Arc;
use ::std::os::raw::c_char;
use ::std::ptr;
use ::std::ffi::{CStr, CString};
use ::std::panic;

use ::jedi::Value;

use ::error::TResult;

/// Init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
}

/// This takes a JSON-encoded object, and parses out the values we care about,
/// and populates them into our app-wide config (overwriting any values we may
/// have set in config.yaml).
fn process_runtime_config(config_str: String) -> TResult<()> {
    let runtime_config: Value = match jedi::parse(&config_str) {
        Ok(x) => x,
        Err(e) => {
            error!("Problem parsing runtime config: {}", e);
            json!({})
        }
    };
    config::merge(&runtime_config)?;
    Ok(())
}

/// Start our app...spawns all our worker/helper threads, including our comm
/// system that listens for external messages.
///
/// NOTE: we have two configs. Our runtime config, which is passed in as a JSON
/// string to start(), and our app config that is loaded on init from our
/// config.yaml file. The runtime config is meant to set up things that will be
/// platform dependent and our UI will most likely have before it even starts
/// the Turtl core.
/// NOTE: we copy the runtime config into our main config, overwriting any of
/// those keys that exist in the config.yaml (app config). this gives the entire
/// app access to our runtime config.
pub fn start(config_str: String) -> thread::JoinHandle<()> {
    info!("main::start() -- init with user config {}", config_str);
    // load our configuration
    process_runtime_config(config_str).unwrap();

    let handle = thread::Builder::new().name(String::from("turtl-main")).spawn(move || {
        let runner = move || -> TResult<()> {
            let data_folder = config::get::<String>(&["data_folder"])?;
            if data_folder != ":memory:" {
                util::create_dir(&data_folder)?;
                info!("main::start() -- created data folder: {}", data_folder);
            }

            // create our turtl object
            let turtl = Arc::new(turtl::Turtl::new()?);

            // start our messaging thread
            let msg_res = messaging::start(move |msg: String| {
                let turtl2 = turtl.clone();
                // spawn a new thread for each message. this lets us process
                // multiple messages at once without blocking.
                let res = thread::Builder::new().name(String::from("dispatch:msg")).spawn(move || {
                    match dispatch::process(turtl2.as_ref(), &msg) {
                        Ok(..) => {},
                        Err(e) => error!("dispatch::process() -- error processing: {}", e),
                    }
                });
                match res {
                    Ok(..) => {},
                    Err(e) => error!("main::start() -- message processor: error spawning thread: {}", e),
                }
            });
            match msg_res {
                Ok(..) => {},
                Err(e) => error!("main::start() -- messaging error: {}", e),
            }
            info!("main::start() -- shutting down");
            Ok(())
        };
        match runner() {
            Ok(_) => (),
            Err(e) => {
                error!("main::start() -- {}", e);
            }
        }
    }).unwrap();

    handle
}

/// Send a message into turtl's dispatcher
pub fn send(msg: String) -> TResult<()> {
    let channel: String = format!("{}-core-in", config::get::<String>(&["messaging", "reqres"])?);
    carrier::send_string(channel.as_str(), msg)?;
    Ok(())
}

fn recv_impl(event: bool, msg_id: Option<&str>) -> TResult<String> {
    let chan_switch = if event { "events" } else { "reqres" };
    let chan_cfg: String = config::get(&["messaging", chan_switch])?;
    let channel: String = match msg_id {
        Some(id) => format!("{}-core-out:{}", chan_cfg, id),
        None => {
            if event {
                chan_cfg
            } else {
                format!("{}-core-out", chan_cfg)
            }
        }
    };
    let msg = carrier::recv(channel.as_str())?;
    Ok(String::from_utf8(msg)?)
}

fn recv_nb_impl(event: bool, msg_id: Option<&str>) -> TResult<Option<String>> {
    let chan_switch = if event { "events" } else { "reqres" };
    let chan_cfg: String = config::get(&["messaging", chan_switch])?;
    let channel: String = match msg_id {
        Some(id) => format!("{}-core-out:{}", chan_cfg, id),
        None => {
            if event {
                chan_cfg
            } else {
                format!("{}-core-out", chan_cfg)
            }
        }
    };
    let msg = carrier::recv_nb(channel.as_str())?;
    let mapped = match msg {
        Some(x) => Some(String::from_utf8(x)?),
        None => None,
    };
    Ok(mapped)
}
/// Receive a turtl message (blocking)
pub fn recv(msg_id: Option<&str>) -> TResult<String> {
    recv_impl(false, msg_id)
}

/// Receive a turtl event (blocking)
pub fn recv_event() -> TResult<String> {
    recv_impl(true, None)
}

/// Receive a turtl message (non-blocking)
pub fn recv_nb(msg_id: Option<&str>) -> TResult<Option<String>> {
    recv_nb_impl(false, msg_id)
}

/// Receive a turtl message (non-blocking)
pub fn recv_event_nb() -> TResult<Option<String>> {
    recv_nb_impl(true, None)
}

// -----------------------------------------------------------------------------
// our C api
// -----------------------------------------------------------------------------

/// Start Turtl
#[no_mangle]
pub extern fn turtlc_start(config_c: *const c_char, threaded: u8) -> i32 {
    let res = panic::catch_unwind(|| -> i32 {
        if config_c.is_null() { return -1; }
        let config_res = unsafe { CStr::from_ptr(config_c).to_str() };
        let config = match config_res {
            Ok(x) => x,
            Err(e) => {
                println!("turtlc_start() -- error: parsing config: {}", e);
                return -3;
            },
        };
        match init() {
            Ok(_) => (),
            Err(e) => {
                println!("turtlc_start() -- error: init(): {}", e);
                return -3;
            },
        }

        let handle = start(String::from(&config[..]));
        if threaded == 0 {
            match handle.join() {
                Ok(_) => (),
                Err(e) => {
                    println!("turtlc_start() -- error: start().join(): {:?}", e);
                    return -4;
                },
            }
        }
        0
    });
    match res {
        Ok(x) => x,
        Err(e) => {
            println!("turtlc_start() -- panic: {:?}", e);
            return -5;
        },
    }
}

#[no_mangle]
pub extern fn turtlc_send(message_bytes: *const u8, message_len: usize) -> i32 {
    let channel: String = match config::get(&["messaging", "reqres"]) {
        Ok(x) => x,
        Err(e) => {
            error!("turtlc_send() -- problem grabbing address (messaging.reqres) from config: {}", e);
            return -5;
        }
    };
    let cstr = match CString::new(format!("{}-core-in", channel)) {
        Ok(x) => x,
        Err(e) => {
            error!("turtlc_send() -- bad channel passed: {}", e);
            return -6;
        }
    };
    carrier::c::carrier_send(cstr.as_ptr(), message_bytes, message_len)
}

fn turtlc_recv_any(non_block: u8, event: u8, msgid_c: *const c_char, len_c: *mut usize) -> *const u8 {
    let null = ptr::null_mut();
    let non_block = non_block == 1;
    let is_ev = event == 1;
    let chan_switch = if is_ev { "events" } else { "reqres" };
    let channel: String = match config::get(&["messaging", chan_switch]) {
        Ok(x) => x,
        Err(e) => {
            error!("turtlc_recv() -- problem grabbing address (messaging.reqres) from config: {}", e);
            return null;
        }
    };
    let suffix = if msgid_c.is_null() {
        ""
    } else {
        let cstr_suffix = unsafe { CStr::from_ptr(msgid_c).to_str() };
        match cstr_suffix {
            Ok(x) => x,
            Err(e) => {
                error!("turtlc_recv() -- bad suffix given: {}", e);
                return null;
            }
        }
    };
    let suffix = if suffix == "" { String::from("") } else { format!(":{}", suffix) };
    let append = if is_ev { "" } else { "-core-out" };
    let channel = format!("{}{}{}", channel, append, suffix);
    let cstr = match CString::new(channel) {
        Ok(x) => x,
        Err(e) => {
            error!("turtlc_recv() -- bad channel passed: {}", e);
            return null;
        }
    };
    if non_block {
        carrier::c::carrier_recv_nb(cstr.as_ptr(), len_c)
    } else {
        carrier::c::carrier_recv(cstr.as_ptr(), len_c)
    }
}

#[no_mangle]
pub extern fn turtlc_recv(non_block: u8, msgid_c: *const c_char, len_c: *mut usize) -> *const u8 {
    turtlc_recv_any(non_block, 0, msgid_c, len_c)
}

#[no_mangle]
pub extern fn turtlc_recv_event(non_block: u8, len_c: *mut usize) -> *const u8 {
    turtlc_recv_any(non_block, 1, ptr::null(), len_c)
}

#[no_mangle]
pub extern fn turtlc_free(msg: *const u8, len: usize) -> i32 {
    carrier::c::carrier_free(msg, len)
}

// -----------------------------------------------------------------------------
// our STUPID JAVA API
// -----------------------------------------------------------------------------
#[cfg(feature = "build-jni")]
#[allow(non_snake_case)]
pub mod android {
    extern crate jni;

    use super::*;
    use self::jni::JNIEnv;
    use self::jni::objects::{JObject, JClass, JString};
    use self::jni::sys::{jint, jbyteArray};
    use ::std::ffi::CString;
    use ::std::slice;

    macro_rules! to_c_string {
        ($fn:expr, $env:ident, $str:ident, $ret:expr) => {{
            let rust_string: String = match $env.get_string($str) {
                Ok(x) => x.into(),
                Err(e) => {
                    println!("{} -- error converting config string: {}", $fn, e);
                    return $ret;
                }
            };
            match CString::new(rust_string) {
                Ok(x) => x,
                Err(e) => {
                    println!("{} -- error converting string to C type: {}", $fn, e);
                    return $ret;
                }
            }
        }}
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_start(env: JNIEnv, _class: JClass, config: JString) -> jint {
        let config_cstring = to_c_string!("main::jni::start()", env, config, -5);
        turtlc_start(config_cstring.as_ptr(), 1)
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_send(env: JNIEnv, _class: JClass, msg: jbyteArray) -> jint {
        let msg_vec = match env.convert_byte_array(msg) {
            Ok(x) => x,
            Err(e) => {
                error!("main::jni::send() -- failed to convert message to vector: {}", e);
                return -5;
            }
        };
        turtlc_send(msg_vec.as_ptr(), msg_vec.len())
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_recv(env: JNIEnv, _class: JClass, mid: JString) -> jbyteArray {
        let null_array = JObject::null().into_inner();
        let mid_cstring = to_c_string!("main::jni::recv()", env, mid, null_array);
        let mut len: usize = 0;
        let raw_len = &mut len as *mut usize;
        let msg_c = turtlc_recv(0, mid_cstring.as_ptr(), raw_len);
        if msg_c.is_null() {
            return null_array;
        }
        let slice = slice::from_raw_parts(msg_c, len);
        let byte_array = match env.byte_array_from_slice(slice) {
            Ok(x) => x,
            Err(e) => {
                error!("main::jni::recv() -- could not convert message to java byte array: {}", e);
                null_array
            }
        };
        turtlc_free(msg_c, len);
        byte_array
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_recv_1nb(env: JNIEnv, _class: JClass, mid: JString) -> jbyteArray {
        let null_array = JObject::null().into_inner();
        let mid_cstring = to_c_string!("main::jni::recv()", env, mid, null_array);
        let mut len: usize = 0;
        let raw_len = &mut len as *mut usize;
        let msg_c = turtlc_recv(1, mid_cstring.as_ptr(), raw_len);
        if msg_c.is_null() {
            return null_array;
        }
        let slice = slice::from_raw_parts(msg_c, len);
        let byte_array = match env.byte_array_from_slice(slice) {
            Ok(x) => x,
            Err(e) => {
                error!("main::jni::recv_nb() -- could not convert message to java byte array: {}", e);
                null_array
            }
        };
        turtlc_free(msg_c, len);
        byte_array
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_recv_1event(env: JNIEnv, _class: JClass) -> jbyteArray {
        let null_array = JObject::null().into_inner();
        let mut len: usize = 0;
        let raw_len = &mut len as *mut usize;
        let msg_c = turtlc_recv_event(0, raw_len);
        if msg_c.is_null() {
            return null_array;
        }
        let slice = slice::from_raw_parts(msg_c, len);
        let byte_array = match env.byte_array_from_slice(slice) {
            Ok(x) => x,
            Err(e) => {
                error!("main::jni::recv_event() -- could not convert message to java byte array: {}", e);
                null_array
            }
        };
        turtlc_free(msg_c, len);
        byte_array
    }

    #[no_mangle]
    pub unsafe extern fn Java_com_lyonbros_turtlcore_TurtlCoreNative_recv_1event_1nb(env: JNIEnv, _class: JClass) -> jbyteArray {
        let null_array = JObject::null().into_inner();
        let mut len: usize = 0;
        let raw_len = &mut len as *mut usize;
        let msg_c = turtlc_recv_event(1, raw_len);
        if msg_c.is_null() {
            return null_array;
        }
        let slice = slice::from_raw_parts(msg_c, len);
        let byte_array = match env.byte_array_from_slice(slice) {
            Ok(x) => x,
            Err(e) => {
                error!("main::jni::recv_event() -- could not convert message to java byte array: {}", e);
                null_array
            }
        };
        turtlc_free(msg_c, len);
        byte_array
    }
}

#[cfg(test)]
#[cfg(feature = "public-api-tests")]
mod tests {
    use super::*;
    use ::std::{thread, slice, str};
    use ::std::ffi::CString;

    fn recv_str(mid: &str) -> String {
        let mut len: usize = 0;
        let raw_len = &mut len as *mut usize;
        let msg = if mid == "" {
            turtlc_recv_event(0, raw_len)
        } else {
            let suffix_c = CString::new(mid).unwrap();
            turtlc_recv(0, suffix_c.as_ptr(), raw_len)
        };

        assert!(!msg.is_null());
        let slice = unsafe { slice::from_raw_parts(msg, len) };
        let res_str = str::from_utf8(slice).unwrap();
        let ret = String::from(res_str);
        turtlc_free(msg, len);
        ret
    }

    #[test]
    fn rust_api() {
        let handle = start(String::from(r#"{"messaging":{"reqres_append_mid":true}}"#));

        let blank = recv_nb(Some("0")).unwrap();
        assert_eq!(blank, None);

        send(String::from(r#"["1","ping"]"#)).unwrap();
        let res_msg = recv(Some("1")).unwrap();
        assert_eq!(res_msg, r#"{"e":0,"d":"pong"}"#);
        let res_ev = recv_event().unwrap();
        assert_eq!(res_ev, r#"{"e":"pong","d":null}"#);

        send(String::from(r#"["2","app:shutdown"]"#)).unwrap();
        let res_msg = recv(Some("2")).unwrap();
        assert_eq!(res_msg, r#"{"e":0,"d":{}}"#);

        handle.join().unwrap();

        let handle = start(String::from(r#"{"messaging":{"reqres_append_mid":false}}"#));

        send(String::from(r#"["3","ping"]"#)).unwrap();
        let res_msg = recv(None).unwrap();
        assert_eq!(res_msg, r#"{"id":"3","e":0,"d":"pong"}"#);
        let res_ev = recv_event().unwrap();
        assert_eq!(res_ev, r#"{"e":"pong","d":null}"#);

        send(String::from(r#"["4","app:shutdown"]"#)).unwrap();
        let res_msg = recv(None).unwrap();
        assert_eq!(res_msg, r#"{"id":"4","e":0,"d":{}}"#);

        handle.join().unwrap();
    }

    #[test]
    fn c_api() {
        let handle = thread::spawn(|| {
            let config = String::from("{}");
            let cstr = CString::new(config).unwrap();
            let res = turtlc_start(cstr.as_ptr());
            assert_eq!(res, 0);
        });

        let msg = Vec::from(String::from("[\"1\",\"ping\"]").as_bytes());
        let res = turtlc_send(msg.as_ptr(), msg.len());
        assert_eq!(res, 0);

        let res_msg = recv_str("1");
        assert_eq!(res_msg, r#"{"e":0,"d":"pong"}"#);
        let res_ev = recv_str("");
        assert_eq!(res_ev, r#"{"e":"pong","d":null}"#);

        let msg = Vec::from(String::from("[\"2\",\"app:shutdown\"]").as_bytes());
        let res = turtlc_send(msg.as_ptr(), msg.len());
        assert_eq!(res, 0);
        let res_msg = recv_str("2");
        assert_eq!(res_msg, r#"{"e":0,"d":{}}"#);
        handle.join().unwrap();
    }
}

