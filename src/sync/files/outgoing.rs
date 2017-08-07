use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::storage::Storage;
use ::api::{self, Api, ApiReq};
use ::messaging;
use ::error::{TResult, TError};
use ::models::file::FileData;
use ::models::sync_record::{SyncType, SyncRecord};
use ::std::fs;
use ::std::io::{Read, Write};
use ::jedi::Value;

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

    /// Returns a list of note_ids for notes that have pending file uploads.
    /// This uses the `sync` table.
    fn get_outgoing_file_syncs(&self) -> TResult<Vec<SyncRecord>> {
        let syncs = with_db!{ db, self.db, "FileSyncOutgoing.get_outgoing_files()",
            SyncRecord::find(db, Some(SyncType::File))
        }?;
        let mut final_syncs = Vec::with_capacity(syncs.len());
        for sync in syncs {
            // NOTE: in the normal outgoing sync process, we break. here, we
            // continue. the reason being that file syncs don't necessarily
            // benefit from being run in order like normal outgoing syncs do.
            if sync.frozen { continue; }
            final_syncs.push(sync);
        }
        Ok(final_syncs)
    }

    /// Given a sync record for an outgoing file, find the corresponding file
    /// in our storage folder and stream it to our heroic API.
    fn upload_file(&mut self, sync: &SyncRecord) -> TResult<()> {
        let note_id = &sync.item_id;
        let user_id = {
            let local_config = self.get_config();
            let guard = local_config.read().unwrap();
            match guard.user_id.as_ref() {
                Some(x) => x.clone(),
                None => return Err(TError::MissingField(String::from("FileSyncOutgoing.upload_file() -- sync config `user_id` is None"))),
            }
        };
        let file = FileData::file_finder(Some(&user_id), Some(note_id))?;
        info!("FileSyncOutgoing.upload_file() -- syncing file {:?}", file);

        // define a container function that grabs our file and runs the upload.
        // if anything in here fails, we mark 
        let upload = |this: &FileSyncOutgoing, note_id, file| -> TResult<()> {
            // start our API call to the note file attachment endpoint
            let url = format!("/notes/{}/attachment", note_id);
            let req = ApiReq::new().header("Content-Type", &String::from("application/octet-stream"));
            // get an API stream we can start piping file data into
            let (mut stream, info) = this.api.call_start(api::Method::Put, &url[..], req)?;
            // open our local, unsynced file and start moving it 4K at a time into
            // the API upload stream
            let mut file = fs::File::open(&file)?;
            let mut buf = [0; 4096];
            loop {
                let read = file.read(&mut buf[..])?;
                if read <= 0 { break; }
                let (read_bytes, _) = buf.split_at(read);
                let written = stream.write(read_bytes)?;
                if read != written {
                    return Err(TError::Msg(format!("FileSyncOutgoing.upload_file() -- problem uploading file: grabbed {} bytes, only sent {} wtf wtf lol", read, written)));
                }
            }
            // write all our output and finalize the API call
            stream.flush()?;
            this.api.call_end::<Value>(stream.send(), info)?;
            Ok(())
        };

        match upload(self, &note_id, file) {
            Ok(_) => {}
            Err(e) => {
                // our upload failed? send to our sync failure handler
                with_db!{ db, self.db, "FileSyncOutgoing.upload_file()",
                    SyncRecord::handle_failed_sync(db, sync)?;
                };
                return Err(e);
            }
        }

        // if we're still here, the upload succeeded. remove the sync record so
        // we know to stop trying to upload this file.
        with_db!{ db, self.db, "FileSyncOutgoing.upload_file()", sync.db_delete(db)? };

        // let the UI know how great we are. you will love this app. tremendous
        // app. everyone says so.
        messaging::ui_event("sync:file:uploaded", &json!({"note_id": note_id}))?;
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
        1000
    }

    fn run_sync(&mut self) -> TResult<()> {
        let syncs = self.get_outgoing_file_syncs()?;
        for sync in &syncs {
            self.upload_file(sync)?;
        }
        Ok(())
    }
}

