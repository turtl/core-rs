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
extern crate crossbeam;

#[macro_use]
mod error;
mod config;
#[macro_use]
mod util;
mod messaging;
mod crypto;
mod models;
mod dispatch;
mod turtl;

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
    let handle = thread::spawn(|| {
        dispatch::main(turtl::Turtl::new());
    });
    util::sleep(10);
    match handle.join() {
        Ok(..) => Ok(()),
        Err(_) => Err(TError::Msg(format!("error joining dispatch thread"))),
    }
}

fn queue() -> TResult<()> {
    let queue: crossbeam::sync::MsQueue<String> = crossbeam::sync::MsQueue::new();
    crossbeam::scope(|scope| {
        scope.spawn(|| {
            queue.push(String::from("jazzzz"));
        });
        scope.spawn(|| {
            println!("got: {}", queue.pop());
        });
    });
    Ok(())
}

/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// TODO: when calling this from C, handle all panics, or get rid of panics.
/// see https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
fn main() {
    init().unwrap();
    queue().unwrap();
    start().unwrap();
}

