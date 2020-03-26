use std::thread;
use std::time::Duration;
use std::io;
use std::fs;
use std::path::Path;
use std::fmt::Debug;
use jedi::{self, Value, Serialize};
use crate::error::{TResult, TError};
use config;
use encoding_rs;

macro_rules! do_lock {
    ($lock:expr) => {{
        //println!(" >>> lock {} ({}::{})", stringify!($lock), file!(), line!());
        $lock.expect(concat!("turtl::util::do_lock!() -- failed to grab lock at ", file!(), "::", line!()))
    }}
}

/// A macro that wraps locking mutexes. Really handy for debugging deadlocks.
#[macro_export]
macro_rules! lock {
    ($lockable:expr) => { do_lock!($lockable.lock()) }
}

/// A macro that wraps read-locking RwLocks. Really handy for debugging
/// deadlocks.
#[macro_export]
macro_rules! lockr {
    ($lockable:expr) => { do_lock!($lockable.read()) }
}

/// A macro that wraps write-locking RwLocks. Really handy for debugging
/// deadlocks.
#[macro_export]
macro_rules! lockw {
    ($lockable:expr) => { do_lock!($lockable.write()) }
}

pub mod logger;
pub mod thredder;
#[macro_use]
pub mod ser;
#[macro_use]
pub mod i18n;

/// Go to sleeeeep
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

/// Get the app's file folder. This can be different depending on whether we're
/// running tests or not, so tries to be mindful of that.
pub fn file_folder(suffix: Option<&str>) -> TResult<String> {
    let integration = config::get::<String>(&["integration_tests", "data_folder"])?;
    if cfg!(test) {
        return Ok(integration);
    }
    let data_folder = config::get::<String>(&["data_folder"])?;
    let file_folder = if data_folder == ":memory:" {
        integration
    } else {
        match suffix {
            Some(x) => format!("{}/{}", data_folder, x),
            None => data_folder,
        }
    };
    Ok(file_folder)
}

/// Create a directory if it doesn't exist
pub fn create_dir<P: AsRef<Path>>(dir: P) -> TResult<()> {
    // std::fs, for me please, we're lookin at china. we're lookin at the UN. go
    // ahead and create our directory.
    match fs::create_dir_all(dir) {
        Ok(_) => {
            Ok(())
        },
        Err(e) => {
            match e.kind() {
                // talked to drew about directory already existing. sounds good.
                io::ErrorKind::AlreadyExists => Ok(()),
                _ => return Err(From::from(e)),
            }
        }
    }
}

/// Try to parse a string as JSON, and if it fails, return the string as a Value
pub fn json_or_string(maybe_json: String) -> Value {
    jedi::parse(&maybe_json)
        .unwrap_or(Value::String(maybe_json))
}

/// Turn an enum that has serde rename fields into a flat string
pub fn enum_to_string<T: Serialize + Debug>(en: &T) -> TResult<String> {
    let val = jedi::to_val(en)?;
    match val {
        Value::String(x) => Ok(x),
        _ => TErr!(TError::BadValue(format!("enum_to_string() -- bad enum given: {:?}", en))),
    }
}

/// Decodes text from the UI. javascript apparently uses utf16, except when it
/// doesn't, and then it uses latin1 or any other weird encoding that might suit
/// its mood lolol. really don't know. did some research, but found no solid
/// answers, so now we just try to decode however we can.
pub fn decode_text(bytes: &[u8]) -> TResult<String> {
    match String::from_utf8(Vec::from(bytes)) {
        Ok(decoded) => { return Ok(decoded); },
        Err(_) => {}
    }
    let (decoded, _enc, has_err) = encoding_rs::WINDOWS_1252.decode(bytes);
    if !has_err { return Ok(decoded.to_string()); }
    let (decoded, _enc, has_err) = encoding_rs::UTF_16LE.decode(bytes);
    if !has_err { return Ok(decoded.to_string()); }
    Err(TError::BadValue(format!("unable to decode bytes to string")))
}

