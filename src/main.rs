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

use ::std::sync::mpsc;

use ::error::{TError, TResult};
use ::util::thredder::{Thredder, OpData};

/// Init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
}

/// Start our app...spawns all our worker/helper threads, including our comm
/// system that listens for external messages.
pub fn start() -> TResult<()> {
    let (tx_to_main, rx_main) = mpsc::channel();
    let thredder_api: Thredder = Thredder::new("api", tx_to_main.clone(), 2);
    let api = api::Api::new(String::from("https://api.turtl.it/v2"));

    thredder_api.run(move || {
        api.get("/users")
    }, |data: TResult<OpData>| {
        println!("response! {:?}", data);
    });
    loop {
        debug!("turtl: main thread message loop");
        match rx_main.recv() {
            Ok(x) => {
                x.call_box();
            },
            Err(e) => error!("thread: main: recv error: {}", e),
        }
    }
    /*
    let handle = thread::spawn(|| {
        dispatch::main(turtl::Turtl::new());
    });
    util::sleep(10);
    match handle.join() {
        Ok(..) => Ok(()),
        Err(_) => Err(TError::Msg(format!("error joining dispatch thread"))),
    }
    */
}

/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// TODO: when calling this from C, handle all panics, or get rid of panics.
/// see https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
fn main() {
    init().unwrap();
    start().unwrap();
}

