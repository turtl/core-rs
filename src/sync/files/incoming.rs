use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;

/// Holds the state for incoming files (download)
pub struct FileSyncIncoming {
    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data and
    /// for polling for file records that need uploading.
    db: Arc<RwLock<Option<Storage>>>,
}

impl FileSyncIncoming {
    /// Create a new incoming syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> Self {
        FileSyncIncoming {
            config: config,
            api: api,
            db: db,
        }
    }
}

impl Syncer for FileSyncIncoming {
    fn get_name(&self) -> &'static str {
        "files:incoming"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn get_delay(&self) -> u64 {
        1000
    }

    fn run_sync(&self) -> TResult<()> {
        Ok(())
    }
}


