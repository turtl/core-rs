//! The sync system is responsible for syncing the data we have stored in our
//! local db with the Turtl server we're talking to.
//!
//! The idea is that all changes to data happen locally, and in fact Turtl is
//! capable of working entirely offline, but once we connect to a server we sync
//! all the outstanding changes to/from the server such that every piece of data
//! is synced.
//!
//! That's the goal, anyway.
//!
//! Keep in mind that ALL data that goes through the sync system needs to be
//! either a) public or b) encrypted. The sync system shall not ever touch
//! plaintext private data. Keeping this in mind, the actual API object should
//! be a separate instance from the one used in the Turtl object, and the Sync
//! object should always be in a separate thread from the main Turtl object.

mod incoming;
mod outgoing;
mod sync_model;

use ::std::thread;
use ::std::sync::{Arc, RwLock};

use ::config;

use ::sync::outgoing::SyncOutgoing;
use ::sync::incoming::SyncIncoming;
use ::util;
use ::util::thredder::Pipeline;
use ::error::TResult;
use ::storage::Storage;

/// This holds the configuration for the sync system (whether it's enabled, the
/// current user id/api endpoint, and any other information we need to make
/// informed decisions about syncing).
///
/// Note that this is a separate struct so that it can be shared by *both the
/// sync system and the main thread* without having the sync system live in the
/// main thread itself. This allows the main thread to update the config without
/// having direct access to the sync thread (and conversely, without sync having
/// access to our precious data in the `Turtl` object that lives in the main
/// thread).
pub struct SyncConfig {
    /// Whether or not to quit the sync thread
    pub quit: bool,
    /// Whether or not to run syncing
    pub enabled: bool,
    /// The current logged in user_id
    pub user_id: String,
}

impl SyncConfig {
    /// Create a new SyncConfig instance.
    pub fn new() -> SyncConfig {
        SyncConfig {
            quit: false,
            enabled: false,
            user_id: String::from(""),
        }
    }
}

/// Defines some common functions for our incoming/outgoing sync objects
pub trait Syncer {
    /// Get this syncer's name
    fn get_name(&self) -> &'static str;

    /// Get a copy of the current sync config
    fn get_config(&self) -> Arc<RwLock<SyncConfig>>;

    /// Run the sync operation for this syncer.
    ///
    /// Essentially, this is the meat of the syncer. This is the entry point for
    /// the custom work this syncer does.
    fn run_sync(&self) -> TResult<()>;

    /// Get the delay (in ms) between called to run_sync() for this Syncer
    fn get_delay(&self) -> u64 {
        1000
    }

    /// Check to see if we should quit the thread
    fn should_quit(&self) -> bool {
        let config = self.get_config();
        let guard = config.read().unwrap();
        guard.quit.clone()
    }

    /// Check to see if we're enabled
    fn is_enabled(&self) -> bool {
        let config = self.get_config();
        let guard = config.read().unwrap();
        guard.enabled.clone()
    }

    /// Get our sync_id key (for our k/v store)
    fn sync_key(&self) -> TResult<String> {
        let config = self.get_config();
        let guard = config.read().unwrap();
        let api_endpoint = try!(config::get::<String>(&["api", "endpoint"]));
        Ok(format!("{}:{}", guard.user_id, api_endpoint))
    }

    /// Runs our syncer, with some quick checks on run status.
    fn runner(&self) {
        info!("sync::runner() -- {} main loop", self.get_name());
        while !self.should_quit() {
            let delay = self.get_delay();
            if self.is_enabled() {
                match self.run_sync() {
                    Err(e) => error!("sync::runner() -- {}: {}", self.get_name(), e),
                    _ => (),
                }
                util::sleep(delay);
            } else {
                util::sleep(delay);
            }
        }
    }
}

/// Start our syncing system!
pub fn start(tx_main: Pipeline, config: Arc<RwLock<SyncConfig>>, kv: Arc<Storage>) -> (thread::JoinHandle<()>, thread::JoinHandle<()>, Box<Fn() + 'static + Sync + Send>) {
    // start our outging sync process
    let tx_main_out = tx_main.clone();
    let config_out = config.clone();
    let kv_out = kv.clone();
    let handle_out = thread::spawn(move || {
        let sync = SyncOutgoing::new(tx_main_out, config_out, kv_out);
        sync.runner();
        info!("sync::start() -- outgoing shutting down");
    });

    // start our incoming sync process
    let tx_main_in = tx_main.clone();
    let config_in = config.clone();
    let kv_in = kv.clone();
    let handle_in = thread::spawn(move || {
        let sync = SyncIncoming::new(tx_main_in, config_in, kv_in);
        sync.runner();
        info!("sync::start() -- incoming shutting down");
    });

    let shutdown = move || {
        let mut guard = config.write().unwrap();
        guard.enabled = false;
        guard.quit = true;
    };

    // send back our handles
    (handle_out, handle_in, Box::new(shutdown))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_sure_everything_works_lol() {
    }
}

