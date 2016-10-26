use ::std::sync::{Arc, RwLock};

use ::jedi::{self, Value};

use ::error::TResult;
use ::sync::{SyncConfig, Syncer};
use ::util::thredder::Pipeline;
use ::storage::Storage;
use ::api::Api;

/// Holds the state for data going from turtl -> API (outgoing sync data).
pub struct SyncOutgoing {
    /// The name of our syncer
    name: &'static str,

    /// The message channel to our main thread.
    tx_main: Pipeline,

    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data and
    /// for polling the "outgoing" table for local changes that need to be
    /// synced to our heroic API.
    db: Arc<Storage>,
}

impl SyncOutgoing {
    /// Create a new outgoing syncer
    pub fn new(tx_main: Pipeline, config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<Storage>) -> SyncOutgoing {
        SyncOutgoing {
            name: "outgoing",
            tx_main: tx_main,
            config: config,
            api: api,
            db: db,
        }
    }
}

impl Syncer for SyncOutgoing {
    fn get_name(&self) -> &'static str {
        self.name
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn get_delay(&self) -> u64 {
        1000
    }

    fn run_sync(&self) -> TResult<()> {
        let records = self.db.all("sync_outgoing");
        println!("outoging sync!");
        Ok(())
    }
}

