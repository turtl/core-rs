use ::std::collections::HashMap;
use ::std::sync::{Arc, RwLock};

use ::error::TResult;
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::util::thredder::Pipeline;
use ::storage::Storage;

/// Holds the state for data going from API -> turtl (incoming sync data),
/// including tracking which sync item's we've seen and which we haven't.
pub struct SyncIncoming {
    /// The name of our syncer
    name: &'static str,

    /// The message channel to our main thread.
    tx_main: Pipeline,

    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our key/value store for tracking our state.
    kv: Arc<Storage>,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    trackers: HashMap<String, Box<SyncModel>>,
}

impl SyncIncoming {
    /// Create a new incoming syncer
    pub fn new(tx_main: Pipeline, config: Arc<RwLock<SyncConfig>>, kv: Arc<Storage>) -> SyncIncoming {
        SyncIncoming {
            name: "incoming",
            tx_main: tx_main,
            config: config,
            kv: kv,
            // TODO: populate with our SyncModels...
            trackers: HashMap::new(),
        }
    }
}

impl Syncer for SyncIncoming {
    fn get_name(&self) -> &'static str {
        self.name
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn run_sync(&self) -> TResult<()> {
        println!("incoming sync!");
        Ok(())
    }
}


