extern crate fern;
extern crate time;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate serde_json;
extern crate nanomsg;
#[macro_use]
extern crate lazy_static;
extern crate openssl;
extern crate rustc_serialize as serialize;

#[macro_use]
mod error;
mod config;
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

//use std::env;
#[allow(dead_code)]
fn msgtest() {
    /*
    let args: Vec<_> = env::args().collect();
    if args.len() > 1 {
        let mut msg = String::new();
        messaging::send_recv("[\"ping\"]".to_owned(), &mut msg).unwrap();
        println!("final msg: {}", msg);
        return;
    }
    */
}

