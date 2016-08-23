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
extern crate hyper;
extern crate futures;
extern crate futures_cpupool;
extern crate crossbeam;

#[macro_use]
mod error;
mod config;
#[macro_use]
mod util;
mod messaging;
mod api;
mod storage;
mod crypto;
mod models;
mod dispatch;
mod turtl;

use ::std::thread;
use ::std::sync::{Arc, RwLock};

use ::crossbeam::sync::MsQueue;
use ::futures::Future;

use ::error::{TError, TResult};

/// Init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
}

lazy_static!{
    static ref RUN: RwLock<bool> = RwLock::new(false);
}

/// Determines if the main thread should be running or not.
fn running() -> bool {
    let guard = (*RUN).read();
    match guard {
        Ok(x) => *x,
        Err(_) => false,
    }
}

/// Sets the running state of the main thread
fn set_running(val: bool) {
    let mut guard = (*RUN).write().unwrap();
    *guard = val;
}

/// Start our app...spawns all our worker/helper threads, including our comm
/// system that listens for external messages.
pub fn start() -> thread::JoinHandle<()> {
    set_running(true);
    thread::spawn(|| {
        let queue_main = Arc::new(MsQueue::new());

        // start our messaging thread
        let (tx_msg, handle) = messaging::start(queue_main.clone());

        // create our turtl object
        let turtl = Arc::new(RwLock::new(turtl::Turtl::new(queue_main.clone(), tx_msg)));

        // run any post-init setup turtl needs
        turtl.write().unwrap().api.set_endpoint(String::from("https://api.turtl.it/v2"));

        /*
        turtl.read().unwrap().api.get("/")
            .map(|x| {
                println!("x is {:?}", x);
            })
            .forget();
        */

        // run our main loop. all threads pipe their data/responses into this
        // loop, meaning <main> only has to check one place to grab messages.
        // this creates an event loop of sorts, without all the grossness.
        while running() {
            debug!("turtl: main thread message loop");
            let handler = queue_main.pop();
            handler.call_box(turtl.clone());
        }
        turtl.write().unwrap().shutdown();
        match handle.join() {
            Ok(..) => {},
            Err(e) => error!("main: problem joining message thread: {:?}", e),
        }
    })
}

/// Stop all threads and close down Turtl
pub fn stop() {
    set_running(false);
}

/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// TODO: when calling this from C, handle all panics, or get rid of panics.
/// see https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
fn main() {
    init().unwrap();
    start().join().unwrap();
}

