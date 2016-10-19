extern crate carrier;
extern crate config;
extern crate crossbeam;
extern crate crypto as rust_crypto;
extern crate dumpy;
extern crate fern;
extern crate futures;
extern crate futures_cpupool;
extern crate gcrypt;
extern crate hyper;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
extern crate libc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate rusqlite;
extern crate rustc_serialize as serialize;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate time;

#[macro_use]
mod error;
#[macro_use]
mod util;
mod messaging;
mod api;
mod crypto;
#[macro_use]
mod models;
mod storage;
mod dispatch;
mod turtl;
mod sync;

use ::std::thread;
use ::std::sync::{Arc, RwLock};
use ::std::fs;
use ::std::io::ErrorKind;
use ::std::os::raw::c_char;
use ::std::ffi::CStr;
use ::std::panic;

use ::crossbeam::sync::MsQueue;
use ::jedi::Value;

use ::error::{TError, TResult};
use ::util::event::Emitter;
use ::util::stopper::Stopper;
use ::util::thredder::Pipeline;
use ::sync::SyncConfig;
use ::storage::Storage;
use ::api::Api;

/// Init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
}

lazy_static!{
    static ref RUN: Stopper = Stopper::new();
}

/// Stop all threads and close down Turtl
fn stop(tx: Pipeline) {
    (*RUN).set(false);
    tx.push(Box::new(move |_| {}));
}

/// This takes a JSON-encoded object, and parses out the values we care about,
/// and populates them into our app-wide config (overwriting any values we may
/// have set in config.yaml).
fn process_runtime_config(config_str: String) -> TResult<()> {
    let runtime_config: Value = match jedi::parse(&config_str) {
        Ok(x) => x,
        Err(_) => jedi::obj(),
    };
    let data_folder: String = match jedi::get(&["data_folder"], &runtime_config) {
        Ok(x) => x,
        Err(_) => String::from("/tmp/turtl.sqlite"),
    };
    try!(config::set(&["data_folder"], &data_folder));
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
    (*RUN).set(true);
    thread::Builder::new().name(String::from("turtl-main")).spawn(move || {
        let runner = move || -> TResult<()> {
            // load our ocnfiguration
            try!(process_runtime_config(config_str));

            let data_folder = try!(config::get::<String>(&["data_folder"]));
            match fs::create_dir(&data_folder) {
                Ok(()) => {
                    info!("main::start() -- created data folder: {}", data_folder);
                },
                Err(e) => {
                    match e.kind() {
                        // talked to drew about directory already existing.
                        // sounds good.
                        ErrorKind::AlreadyExists => (),
                        _ => {
                            return Err(From::from(e));
                        }
                    }
                }
            }

            let queue_main = Arc::new(MsQueue::new());

            // start our messaging thread
            let (handle, msg_shutdown) = messaging::start(queue_main.clone());

            let api = Arc::new(Api::new());
            let kv = Arc::new(try!(Storage::new(&format!("{}/kv.sqlite", &data_folder), jedi::obj())));
            let sync_config = Arc::new(RwLock::new(SyncConfig::new()));

            // create our turtl object
            let turtl = try!(turtl::Turtl::new_wrap(queue_main.clone(), api.clone(), kv.clone(), sync_config.clone()));

            // bind turtl.events "app:shutdown" to close everything
            {
                let ref mut events = turtl.write().unwrap().events;
                let tx_main_shutdown = queue_main.clone();
                events.bind("app:shutdown", move |_| {
                    stop(tx_main_shutdown.clone());
                }, "app:shutdown");
            }

            // run our main loop. all threads pipe their data/responses into this
            // loop, meaning <main> only has to check one place to grab messages.
            // this creates an event loop of sorts, without all the grossness.
            info!("main::start() -- main loop");
            while (*RUN).running() {
                debug!("turtl: main thread message loop");
                let handler = queue_main.pop();
                handler.call_box(turtl.clone());
            }
            info!("main::start() -- shutting down");
            turtl.write().unwrap().shutdown();
            msg_shutdown();
            match handle.join() {
                Ok(_) => (),
                Err(e) => {
                    error!("main::start() -- error joining messaging thread: {:?}", e);
                }
            }
            Ok(())
        };
        match runner() {
            Ok(_) => (),
            Err(e) => {
                error!("main::start() -- {}", e);
            }
        }
    }).unwrap()
}

// -----------------------------------------------------------------------------
// our C api
// -----------------------------------------------------------------------------

/// Start Turtl
#[no_mangle]
pub extern fn turtl_start(config_c: *const c_char) -> i32 {
    let res = panic::catch_unwind(|| -> i32 {
        if config_c.is_null() { return -1; }
        let config_res = unsafe { CStr::from_ptr(config_c).to_str() };
        let config = match config_res {
            Ok(x) => x,
            Err(e) => {
                println!("turtl_start() -- error: parsing config: {}", e);
                return -3;
            },
        };
        match init() {
            Ok(_) => (),
            Err(e) => {
                println!("turtl_start() -- error: init(): {}", e);
                return -3;
            },
        }

        match start(String::from(&config[..])).join() {
            Ok(_) => (),
            Err(e) => {
                println!("turtl_start() -- error: start().join(): {:?}", e);
                return -4;
            },
        }
        0
    });
    match res {
        Ok(x) => x,
        Err(e) => {
            println!("turtl_start() -- panic: {:?}", e);
            return -5;
        },
    }
}

