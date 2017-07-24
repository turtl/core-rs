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
//! plaintext private data. Keeping this in mind, the Sync object should always
//! be in a separate thread from the main Turtl object.

mod incoming;
mod outgoing;
#[macro_use]
pub mod sync_model;

use ::std::thread;
use ::std::sync::{Arc, RwLock, mpsc};

use ::config;

use ::sync::outgoing::SyncOutgoing;
use ::sync::incoming::SyncIncoming;
use ::util;
use ::error::TResult;
use ::storage::Storage;
use ::api::Api;
use ::messaging;

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
    /// Whether or not to skip calling out to the API on init (useful for
    /// testing)
    pub skip_api_init: bool,
}

impl SyncConfig {
    /// Create a new SyncConfig instance.
    pub fn new() -> SyncConfig {
        SyncConfig {
            quit: false,
            enabled: false,
            user_id: String::from(""),
            skip_api_init: false,
        }
    }
}

/// A structure that tracks some state for a running sync system.
pub struct SyncState {
    pub join_handles: Vec<thread::JoinHandle<()>>,
    pub shutdown: Box<Fn() + 'static + Sync + Send>,
    pub pause: Box<Fn() + 'static + Sync + Send>,
    pub resume: Box<Fn() + 'static + Sync + Send>,
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

    /// Run any initialization this Syncer needs.
    fn init(&self) -> TResult<()> {
        Ok(())
    }

    /// Get the delay (in ms) between called to run_sync() for this Syncer
    fn get_delay(&self) -> u64 {
        1000
    }

    /// Check to see if we should quit the thread
    fn should_quit(&self) -> bool {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        guard.quit.clone()
    }

    /// Check to see if we're enabled
    fn is_enabled(&self) -> bool {
        let config_enabled_res = if self.get_name() == "outgoing" {
            config::get(&["sync", "enable_outgoing"])
        } else {
            config::get(&["sync", "enable_incoming"])
        };
        let config_enabled: bool = match config_enabled_res {
            Ok(x) => x,
            Err(_) => true,
        };
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        guard.enabled.clone() && config_enabled
    }

    /// Get our sync_id key (for our k/v store)
    fn sync_key(&self) -> TResult<String> {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        let api_endpoint = config::get::<String>(&["api", "endpoint"])?;
        Ok(format!("{}:{}", guard.user_id, api_endpoint))
    }

    /// Runs our syncer, with some quick checks on run status.
    fn runner(&self, init_tx: mpsc::Sender<TResult<()>>) {
        info!("sync::runner() -- {} init", self.get_name());
        let init_res = self.init();
        macro_rules! send_or_return {
            ($sendex:expr) => {
                match $sendex {
                    Err(e) => error!("sync::{}::runner() -- problem sending init signal: {}", self.get_name(), e),
                    _ => (),
                }
            }
        }
        match init_res {
            Ok(_) => {
                send_or_return!(init_tx.send(Ok(())));
            },
            Err(e) => {
                error!("sync::runner() -- {}: init: {}", self.get_name(), e);
                send_or_return!(init_tx.send(Err(e)));
                return;
            },
        }
        match init_tx.send(init_res) {
            Err(e) => error!("sync::{}::runner() -- problem sending init signal: {}", self.get_name(), e),
            _ => (),
        }

        info!("sync::runner() -- {} main loop", self.get_name());
        while !self.should_quit() {
            let delay = self.get_delay();
            if self.is_enabled() {
                match self.run_sync() {
                    Err(e) => error!("sync::runner() -- {}: main loop: {}", self.get_name(), e),
                    _ => (),
                }
                util::sleep(delay);
            } else {
                util::sleep(delay);
            }
        }
    }

    /// Let the main thread know that we've (dis)connected to the API. Useful
    /// for updating the UI on our connection state
    fn connected(&self, yesno: bool) {
        messaging::ui_event("sync:connected", &yesno)
            .unwrap_or_else(|e| error!("Syncer::connected() -- error sending connected event: {}", e));
    }
}

/// Start our syncing system!
///
/// Note that we have separate db objects for in/out. This is because each
/// thread needs its own connection. We don't have the ability to create the
/// connections in this scope (no access to Turtl by design) so we need to
/// just have them passed in.
pub fn start(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db_out: Storage, db_in: Storage) -> TResult<SyncState> {
    // enable syncing (set phasers to stun)
    {
        let mut config_guard = config.write().unwrap();
        (*config_guard).enabled = true;
        (*config_guard).quit = false;
    }

    // start our outging sync process
    let config_out = config.clone();
    let (tx_out, rx_out) = mpsc::channel::<TResult<()>>();
    let api_out = api.clone();
    let handle_out = thread::Builder::new().name(String::from("sync:outgoing")).spawn(move || {
        let sync = SyncOutgoing::new(config_out, api_out, db_out);
        sync.runner(tx_out);
        info!("sync::start() -- outgoing shut down");
    })?;

    // start our incoming sync process
    let config_in = config.clone();
    let (tx_in, rx_in) = mpsc::channel::<TResult<()>>();
    let api_in = api.clone();
    let handle_in = thread::Builder::new().name(String::from("sync:incoming")).spawn(move || {
        let sync = SyncIncoming::new(config_in, api_in, db_in);
        sync.runner(tx_in);
        info!("sync::start() -- incoming shut down");
    })?;

    macro_rules! channel_check {
        ($rx:expr) => {
            match $rx {
                Ok(x) => {
                    match x {
                        Err(e) => return Err(toterr!(e)),
                        _ => (),
                    }
                },
                Err(e) => return Err(toterr!(e)),
            }
        }
    }
    channel_check!(rx_out.recv());
    channel_check!(rx_in.recv());

    let config1 = config.clone();
    let shutdown = move || {
        let mut guard = config1.write().unwrap();
        guard.enabled = false;
        guard.quit = true;
    };
    let config2 = config.clone();
    let pause = move || {
        let mut guard = config2.write().unwrap();
        guard.enabled = false;
    };
    let config3 = config.clone();
    let resume = move || {
        let mut guard = config3.write().unwrap();
        guard.enabled = true;
    };

    Ok(SyncState {
        join_handles: vec![handle_out, handle_in],
        shutdown: Box::new(shutdown),
        pause: Box::new(pause),
        resume: Box::new(resume),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use ::std::sync::{Arc, RwLock};

    use ::jedi::{self, Value};
    use ::storage::Storage;
    use ::api::Api;
    use ::models::sync_record::SyncRecord;

    #[test]
    fn serializes_sync_record() {
        let sync: SyncRecord = jedi::parse(&String::from(r#"{"id":1234,"user_id":1,"item_id":6969,"action":"add","type":"note","data":{"id":"6969"}}"#)).unwrap();
        assert_eq!(sync.id, Some(String::from("1234")));
        assert_eq!(sync.action, String::from("add"));
        assert_eq!(sync.sync_ids, None);
        assert_eq!(sync.ty, String::from("note"));
        let data: Value = jedi::to_val(&sync.data).unwrap();
        assert_eq!(jedi::get::<String>(&["id"], &data).unwrap(), String::from(r#"6969"#));

        let syncstr: String = jedi::stringify(&sync).unwrap();
        assert_eq!(syncstr, String::from(r#"{"id":"1234","body":null,"action":"add","item_id":"6969","user_id":1,"type":"note","data":{"id":"6969"}}"#));
    }

    #[test]
    fn starts_and_quits() {
        let mut sync_config = SyncConfig::new();
        sync_config.skip_api_init = true;
        let sync_config = Arc::new(RwLock::new(sync_config));
        let api = Arc::new(Api::new());
        let db_out = Storage::new(&String::from(":memory:"), jedi::obj()).unwrap();
        let db_in = Storage::new(&String::from(":memory:"), jedi::obj()).unwrap();
        let mut state = start(sync_config, api, db_out, db_in).unwrap();
        (state.shutdown)();
        loop {
            let hn = state.join_handles.pop();
            match hn {
                Some(x) => x.join().unwrap(),
                None => break,
            }
        }
    }
}

