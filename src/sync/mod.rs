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

#[macro_use]
mod macros;
mod incoming;
pub mod outgoing;
pub mod files;
#[macro_use]
pub mod sync_model;

use ::std::thread;
use ::std::sync::{Arc, RwLock, mpsc};

use ::config;

use ::sync::outgoing::SyncOutgoing;
use ::sync::incoming::SyncIncoming;
use ::sync::files::outgoing::FileSyncOutgoing;
use ::sync::files::incoming::FileSyncIncoming;
use ::util;
use ::error::{TResult, TError};
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
    pub user_id: Option<String>,
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
            user_id: None,
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
        let user_id = match guard.user_id.as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("Syncer.sync_key() -- sync config `user_id` is None"))),
        };
        Ok(format!("{}:{}", user_id, api_endpoint))
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
pub fn start(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> TResult<SyncState> {
    // enable syncing (set phasers to stun)
    {
        let mut config_guard = config.write().unwrap();
        (*config_guard).enabled = true;
        (*config_guard).quit = false;
    }

    // some holders for our thread handles and init receivers
    let mut join_handles = Vec::with_capacity(4);
    let mut rx_vec = Vec::with_capacity(4);

    /// Starts a sync class.
    macro_rules! sync_starter {
        ($synctype:expr) => {
            {
                // create the channel we'll use to send messages from the sync
                // thread back to here (mainly, a "yes init succeeded" or "no,
                // init failed")
                let (tx, rx) = mpsc::channel::<TResult<()>>();
                let config_c = config.clone();
                let api_c = api.clone();
                let db_c = db.clone();
                let sync = $synctype(config_c, api_c, db_c);
                let handle = thread::Builder::new().name(format!("sync:{}", sync.get_name())).spawn(move || {
                    sync.runner(tx);
                    info!("sync::start() -- {} shut down", sync.get_name());
                })?;
                // push our handle/rx onto their respective holder vecs
                join_handles.push(handle);
                rx_vec.push(rx);
            }
        }
    }

    // i try to use the type without the ::new but drew *destroy the value* of
    // the macro!
    sync_starter!(SyncOutgoing::new);
    sync_starter!(SyncIncoming::new);
    sync_starter!(FileSyncOutgoing::new);
    sync_starter!(FileSyncIncoming::new);

    // Wait on an "OK! A++++" Ok(()) signal from the sync thread (sent after it
    // inits successfully) or a "SHITFUCK!" Err() if there was a problem.
    for rx in rx_vec {
        match rx.recv() {
            Ok(x) => {
                match x {
                    Err(e) => return Err(toterr!(e)),
                    _ => (),
                }
            },
            Err(e) => return Err(toterr!(e)),
        }
    }

    // define some callbacks Turtl can use to control the sync processes. turtl
    // could manage this junk itself, but it's nicer to have a single object
    // that handles the state for us via functions.
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

    // uhhh, you have a ffcall. thank you. uhh, hand the phone to me, please.
    // yes, here you go.
    Ok(SyncState {
        join_handles: join_handles,
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
    use ::models::sync_record::{SyncAction, SyncType, SyncRecord};

    #[test]
    fn serializes_sync_record() {
        let sync: SyncRecord = jedi::parse(&String::from(r#"{"id":1234,"user_id":1,"item_id":6969,"action":"add","type":"note","data":{"id":"6969"}}"#)).unwrap();
        assert_eq!(sync.id, Some(String::from("1234")));
        assert_eq!(sync.action, SyncAction::Add);
        assert_eq!(sync.sync_ids, None);
        assert_eq!(sync.ty, SyncType::Note);
        let data: Value = jedi::to_val(&sync.data).unwrap();
        assert_eq!(jedi::get::<String>(&["id"], &data).unwrap(), String::from(r#"6969"#));

        let syncstr: String = jedi::stringify(&sync).unwrap();
        assert_eq!(syncstr, String::from(r#"{"id":"1234","body":null,"action":"add","item_id":"6969","user_id":1,"type":"note","data":{"id":"6969"},"errcount":0,"frozen":false}"#));
    }

    #[test]
    fn starts_and_quits() {
        let mut sync_config = SyncConfig::new();
        sync_config.skip_api_init = true;
        let sync_config = Arc::new(RwLock::new(sync_config));
        let api = Arc::new(Api::new());
        let db = Arc::new(RwLock::new(Some(Storage::new(&String::from(":memory:"), jedi::obj()).unwrap())));
        let mut state = start(sync_config, api, db).unwrap();
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

