extern crate carrier;
extern crate clouseau;
extern crate config;
extern crate crossbeam;
extern crate crypto as rust_crypto;
extern crate dumpy;
extern crate encoding;
extern crate fern;
extern crate futures;
extern crate futures_cpupool;
extern crate gcrypt;
extern crate hyper;
#[macro_use]
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate num_cpus;
#[macro_use]
extern crate quick_error;
extern crate regex;
extern crate rusqlite;
extern crate rustc_serialize as serialize;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate time;

#[macro_use]
pub mod error;
#[macro_use]
mod util;
mod messaging;
mod api;
mod crypto;
#[macro_use] mod sync;
#[macro_use]
mod models;
mod profile;
mod storage;
mod search;
mod dispatch;
mod turtl;

use ::std::thread;
use ::std::fs;
use ::std::io::ErrorKind;
use ::std::os::raw::c_char;
use ::std::ffi::CStr;
use ::std::panic;

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
    tx.next(|_| {});
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
        Err(_) => String::from("/tmp/"),
    };
    config::set(&["data_folder"], &data_folder)?;
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
            process_runtime_config(config_str)?;

            // std::fs, for me please, we're lookin at china. we're lookin at
            // the UN. go ahead and create our data directory.
            let data_folder = config::get::<String>(&["data_folder"])?;
            if data_folder != ":memory:" {
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
            }

            // create our main "Pipeline" ...this is what all our threads use to
            // send massages to the main thread.
            let tx_main = Pipeline::new();

            // start our messaging thread
            let (handle_msg, msg_shutdown) = messaging::start(tx_main.clone())?;

            // create our turtl object
            let turtl = turtl::Turtl::new_wrap(tx_main.clone())?;

            // bind turtl.events "app:shutdown" to close everything
            {
                let tx_main_shutdown = tx_main.clone();
                turtl.events.bind("app:shutdown", move |_| {
                    stop(tx_main_shutdown.clone());
                }, "app:shutdown:main");
            }

            // run our main loop. all threads pipe their data/responses into this
            // loop, meaning <main> only has to check one place to grab messages.
            // this creates an event loop of sorts, without all the grossness.
            info!("main::start() -- main loop");
            while (*RUN).running() {
                debug!("turtl: main thread message loop");
                let handler = tx_main.pop();
                handler.call_box(turtl.clone());
            }
            info!("main::start() -- shutting down");

            // shut down the messaging system
            msg_shutdown();
            handle_msg.join()?;

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

