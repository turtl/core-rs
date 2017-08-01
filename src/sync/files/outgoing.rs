use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{self, Api, ApiReq};
use ::messaging;
use ::error::{TResult, TError};
use ::models::file::{FileData, FileSyncStatus};
use ::std::path::PathBuf;
use ::std::fs;
use ::std::io::{Read, Write};

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
    fn get_unsynced_files(&self) -> TResult<Vec<PathBuf>> {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        let user_id = match guard.user_id.as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("FileSyncOutgoing.get_unsynced_files() -- sync config `user_id` is None"))),
        };
        FileData::file_finder_all(Some(user_id), None, Some(FileSyncStatus::Unsynced))
    }

    /// Stream a local file up to our heroic API
    fn upload_file(&self, file: PathBuf) -> TResult<()> {
        let note_id: String = FileData::get_note_id(&file)?;
        FileData::set_sync_status(&note_id, FileSyncStatus::Syncing)?;
        let url = format!("/notes/{}/attachment", note_id);
        let mut req = ApiReq::new();
        let (mut stream, info) = self.api.call_start(api::Method::Put, &url[..], ApiReq::new())?;
        let mut file = fs::File::open(&file)?;
        let mut buf = [0; 4096];
        loop {
            let read = file.read(&mut buf[..])?;
            if read <= 0 { break; }
            let written = stream.write(&buf)?;
            if read != written {
                return Err(TError::Msg(format!("FileSyncOutgoing.upload_file() -- problem uploading file: grabbed {} bytes, only sent {}", read, written)));
            }
        }
        let res = self.api.call_end(stream.send(), info)?;
        Ok(())
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
        for file in files {
            self.upload_file(file)?;
        }
        Ok(())
    }
}

