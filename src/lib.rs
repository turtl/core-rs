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
use ::std::sync::Arc;
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

struct RuntimeConfig {
    data_folder: String,
}

/// This takes a JSON-encoded object, and parses out the values we care about
/// into a `RuntimeConfig` struct which can be used to configure various parts
/// of the app.
fn process_runtime_config(config_str: String) -> RuntimeConfig {
    let runtime_config: Value = match jedi::parse(&config_str) {
        Ok(x) => x,
        Err(_) => jedi::obj(),
    };
    let data_folder: String = match jedi::get(&["data_folder"], &runtime_config) {
        Ok(x) => x,
        Err(_) => String::from("/tmp/turtl.sql"),
    };
    RuntimeConfig {
        data_folder: data_folder,
    }
}

/// Start our app...spawns all our worker/helper threads, including our comm
/// system that listens for external messages.
///
/// NOTE: we have two configs. Our runtime config, which is passed in as a JSON
/// string to start(), and our app config that is loaded on init from our
/// config.yaml file.
///
/// The runtime config is meant to set up things that will be platform
/// independent and our UI will most like have before it even starts the Turtl
/// core. This includes
///
/// The app config is meant to provide everything else.
pub fn start(config_str: String) -> thread::JoinHandle<()> {
    (*RUN).set(true);
    thread::Builder::new().name(String::from("turtl-main")).spawn(move || {
        let runner = move || -> TResult<()> {
            // load our ocnfiguration
            let runtime_config: RuntimeConfig = process_runtime_config(config_str);

            match fs::create_dir(&runtime_config.data_folder[..]) {
                Ok(()) => {
                    info!("main::start() -- created data folder: {}", runtime_config.data_folder);
                },
                Err(e) => {
                    match e.kind() {
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

            // grab our messaging channel from config
            let msg_channel: String = match config::get(&["messaging", "address"]) {
                Ok(x) => x,
                Err(e) => {
                    error!("messaging: problem grabbing address (messaging.address) from config, using default: {}", e);
                    String::from("inproc://turtl")
                }
            };

            let dumpy_schema = try!(config::get::<Value>(&["schema"]));

            // create our turtl object
            let turtl = try!(turtl::Turtl::new_wrap(queue_main.clone(), msg_channel, &runtime_config.data_folder, dumpy_schema));

            // bind turtl.events "app:shutdown" to close everything
            {
                let ref mut events = turtl.write().unwrap().events;
                let tx_main_shutdown = queue_main.clone();
                events.bind("app:shutdown", move |_| {
                    stop(tx_main_shutdown.clone());
                }, "app:shutdown");
            }

            // set our default api endpoint
            turtl.write().unwrap().api.set_endpoint(&try!(config::get(&["api", "endpoint"])));

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

