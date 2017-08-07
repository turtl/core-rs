use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::storage::Storage;
use ::api::{Api, ApiReq, Method, Headers};
use ::messaging;
use ::error::{TResult, TError};
use ::models::sync_record::{SyncType, SyncRecord};
use ::models::file::FileData;
use ::hyper;
use ::std::time::Duration;
use ::std::fs;
use ::std::io::{Read, Write};

/// Holds the state for incoming files (download)
pub struct FileSyncIncoming {
    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data and
    /// for polling for file records that need downloading.
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

    /// Returns a list of note_ids for notes that have pending file downloads.
    /// This uses the `sync` table.
    fn get_incoming_file_syncs(&self) -> TResult<Vec<SyncRecord>> {
        let syncs = with_db!{ db, self.db, "FileSyncIncoming.get_incoming_file_syncs()",
            SyncRecord::find(db, Some(SyncType::FileIncoming))
        }?;
        let mut final_syncs = Vec::with_capacity(syncs.len());
        for sync in syncs {
            // NOTE: in the normal sync process, we break on frozen. here, we
            // continue. the reason being that file syncs don't necessarily
            // benefit from being run in order like normal outgoing syncs do.
            if sync.frozen { continue; }
            final_syncs.push(sync);
        }
        Ok(final_syncs)
    }

    /// Given a sync record for an outgoing file, find the corresponding file
    /// in our storage folder and stream it to our heroic API.
    fn download_file(&mut self, sync: &SyncRecord) -> TResult<()> {
        let note_id = &sync.item_id;
        let user_id = {
            let local_config = self.get_config();
            let guard = local_config.read().unwrap();
            match guard.user_id.as_ref() {
                Some(x) => x.clone(),
                None => return Err(TError::MissingField(String::from("FileSyncIncoming.download_file() -- sync config `user_id` is None"))),
            }
        };
        info!("FileSyncIncoming.download_file() -- syncing file for {}", note_id);

        // define a container function that grabs our file and runs the download.
        // if anything in here fails, we mark 
        let download = |note_id, user_id| -> TResult<()> {
            // generate the filename we'll save to, and open the file (we should
            // test if the file can be created before we run off blasting API
            // calls in every direction)
            let file = FileData::new_file(user_id, note_id)?;
            let mut file = fs::File::create(&file)?;

            // start our API call to the note file attachment endpoint
            let url = format!("/notes/{}/attachment", note_id);
            // grab the location of the file we'll be downloading
            let file_url: String = self.api.get(&url[..], ApiReq::new())?;
            let mut headers = Headers::new();
            self.api.set_auth_headers(&mut headers);
            let mut client = hyper::Client::new();
            client.set_read_timeout(Some(Duration::new(30, 0)));
            let mut res = client
                .request(Method::Get, &file_url[..])
                .headers(headers)
                .send()?;
            // start streaming our API call into the file 4K at a time
            let mut buf = [0; 4096];
            loop {
                let read = res.read(&mut buf[..])?;
                // all done! (EOF)
                if read <= 0 { break; }
                let (read_bytes, _) = buf.split_at(read);
                let written = file.write(read_bytes)?;
                if read != written {
                    return Err(TError::Msg(format!("FileSyncIncoming.download_file() -- problem downloading file: downloaded {} bytes, only saved {} wtf wtf lol", read, written)));
                }
            }
            Ok(())
        };

        match download(&note_id, &user_id) {
            Ok(_) => {}
            Err(e) => {
                // our download failed? send to our sync failure handler
                with_db!{ db, self.db, "FileSyncIncoming.download_file()",
                    SyncRecord::handle_failed_sync(db, sync)?;
                };
                return Err(e);
            }
        }

        // if we're still here, the download succeeded. remove the sync record so
        // we know to stop trying to download this file.
        with_db!{ db, self.db, "FileSyncIncoming.download_file()", sync.db_delete(db, None)? };

        // let the UI know how great we are. you will love this app. tremendous
        // app. everyone says so.
        messaging::ui_event("sync:file:downloaded", &json!({"note_id": note_id}))?;
        Ok(())
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

    fn run_sync(&mut self) -> TResult<()> {
        let syncs = self.get_incoming_file_syncs()?;
        for sync in &syncs {
            self.download_file(sync)?;
        }
        Ok(())
    }
}


