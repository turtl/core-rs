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
extern crate hyper;

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
use ::std::sync::{mpsc};
use ::std::io::Read;

use ::error::{TError, TResult};
use ::util::thredder;
use ::util::reqres::ReqRes;

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
    let (tx_to_main, rx_main) = mpsc::channel();
    let reqres = ReqRes::new();
    //let thredder_messaging = thredder::spawn("messaging", tx_to_main.clone(), messaging::dispatch);
    let thredder_api = thredder::spawn("api", tx_to_main.clone(), api::dispatch);
    //let thredder_storage = thredder::spawn("storage", tx_to_main.clone(), storage::dispatch);
    //let (tx_to_worker, handle_worker) = thredder::spawn();

    loop {
        match rx_main.recv() {
            Ok(x) => {
                //recres.
            },
            Err(e) => error!("thread: main: recv error: {}", e),
        }
    }
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
            let data = String::from("get a job");
            println!("addr: thread: {:p}", &(data.as_bytes()[0]));
            queue.push(data);
        });
        /*
        scope.spawn(|| {
            println!("got: {:?}", queue.pop());
        });
        */
    });
    let data = queue.pop();
    println!("addr: thread: {:p}", &(data.as_bytes()[0]));
    println!("got: {:?}", data);
    Ok(())
}

fn http() -> TResult<()> {
    let client = hyper::Client::new();
    let mut out = String::new();
    let mut res = try_t!(client.get("https://api.turtl.it/v2").send());
    res.read_to_string(&mut out);
    println!("res {}", out);
    Ok(())
}

fn send() -> TResult<()> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let data: Vec<u8> = vec![1,2,3,4,5];
        println!("addr: thread: {:p} -- {:p}", &data, &data[0]);
        tx.send(data).unwrap();
    });
    let data = rx.recv().unwrap();
    println!("addr: main: {:p} -- {:p}", &data, &data[0]);
    println!("got: {}", data[0]);
    Ok(())
}

/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
/// TODO: when calling this from C, handle all panics, or get rid of panics.
/// see https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
/// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
fn main() {
    init().unwrap();
    queue().unwrap();
    //http().unwrap();
    //send().unwrap();
    start().unwrap();
}

