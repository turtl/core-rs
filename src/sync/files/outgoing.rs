use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::error::{TResult, TError};
use ::models::file::{FileData, FileSyncStatus};
use ::std::path::PathBuf;

/// Holds the state for outgoing files (uploads)
pub struct FileSyncOutgoing {
    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data and
    /// for polling for file records that need uploading.
    db: Arc<RwLock<Option<Storage>>>,
}

impl FileSyncOutgoing {
    /// Create a new outgoing syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> Self {
        FileSyncOutgoing {
            config: config,
            api: api,
            db: db,
        }
    }

    /// Returns a list of files that are wating to be synced to the servers
    pub fn get_unsynced_files(&self) -> TResult<Vec<PathBuf>> {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        let user_id = match guard.user_id.as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("FileSyncOutgoing.get_unsynced_files() -- sync config `user_id` is None"))),
        };
        FileData::file_finder_all(Some(user_id), None, Some(FileSyncStatus::Unsynced))
    }
}

impl Syncer for FileSyncOutgoing {
    fn get_name(&self) -> &'static str {
        "files:outgoing"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn get_delay(&self) -> u64 {
        5000
    }

    fn run_sync(&self) -> TResult<()> {
        let files = self.get_unsynced_files()?;

        Ok(())
    }
}

