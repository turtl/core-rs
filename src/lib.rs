extern crate carrier;
extern crate clouseau;
extern crate config;
extern crate crossbeam;
extern crate dumpy;
extern crate encoding;
extern crate fern;
extern crate futures;
extern crate futures_cpupool;
extern crate hyper;
#[macro_use]
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
//extern crate migrate;
extern crate num_cpus;
#[macro_use]
extern crate protected_derive;
#[macro_use]
extern crate quick_error;
extern crate regex;
extern crate rusqlite;
extern crate rustc_serialize as serialize;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate sodiumoxide;
extern crate time;

#[macro_use]
pub mod error;
#[macro_use]
mod util;
mod crypto;
mod messaging;
mod api;
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

use ::error::TResult;
use ::util::event::Emitter;

/// Init any state/logging/etc the app needs
pub fn init() -> TResult<()> {
    match util::logger::setup_logger() {
        Ok(..) => Ok(()),
        Err(e) => Err(toterr!(e)),
    }
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

            // create our turtl object
            let turtl = turtl::Turtl::new_wrap()?;
            turtl.events.bind("app:shutdown", |_| {
                messaging::stop();
            }, "app:shutdown:main");

            // start our messaging thread
            let msg_res = messaging::start(move |msg: String| {
                let turtl2 = turtl.clone();
                let res = thread::Builder::new().name(String::from("dispatch:msg")).spawn(move || {
                    match dispatch::process(turtl2, &msg) {
                        Ok(..) => {},
                        Err(e) => error!("dispatch::process() -- error processing: {}", e),
                    }
                });
                match res {
                    Ok(..) => {},
                    Err(e) => error!("main::start() -- message processor: error spawning thread: {}", e),
                }
            });
            match msg_res {
                Ok(..) => {},
                Err(e) => error!("main::start() -- messaging error: {}", e),
            }
            info!("main::start() -- shutting down");
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

