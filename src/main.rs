extern crate fern;
extern crate time;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate nanomsg;
#[macro_use]
extern crate lazy_static;
extern crate rustc_serialize as serialize;
extern crate gcrypt;
extern crate crypto as rust_crypto;
extern crate constant_time_eq;

#[macro_use]
mod error;
mod config;
#[macro_use]
mod util;
mod messaging;
mod crypto;
mod models;
mod dispatch;

use std::thread;

use error::{TError, TResult};

/// init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
}

/// start our app. basically, start listening for incoming messages on a new
/// thread and process them
pub fn start() -> TResult<()> {
    let handle = thread::spawn(dispatch::main);
    util::sleep(10);
    //let msg = r#"["user:login",{"username":"andrew","password":"passsss"}]"#;
    //messaging::send(&msg.to_owned()).unwrap();
    match handle.join() {
        Ok(..) => Ok(()),
        Err(_) => Err(TError::Msg(format!("error joining dispatch thread"))),
    }
}

/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// TODO: when calling this from C, handle all panics, or get rid of panics.
/// see https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
fn main() {
    init().unwrap();
    start().unwrap();
}

