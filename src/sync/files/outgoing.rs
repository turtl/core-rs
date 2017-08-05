use ::std::sync::{Arc, RwLock};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{self, Api, ApiReq};
use ::messaging;
use ::error::{TResult, TError};
use ::models::file::FileData;
use ::models::file_sync::{FileSyncType, FileSync};
use ::std::path::PathBuf;
use ::std::fs;
use ::std::io::{Read, Write};
use ::jedi::{self, Value};
use ::std::collections::HashMap;

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
    /// This uses the `file_sync` table, which basically just holds type/note_id
    /// and acts as a sort of "queue" for file operations.
    fn get_outgoing_files(&self) -> TResult<Vec<String>> {
        let recs: Vec<FileSync> = with_db!{ db, self.db, "FileSyncOutgoing.get_outgoing_files()",
            db.find("file_sync", "type", &vec![jedi::parse::<String>(&jedi::stringify(&FileSyncType::Outgoing)?)?])?
        };
        let mut note_ids: Vec<String> = Vec::with_capacity(recs.len());
        for rec in recs {
            let note_id = match rec.id.as_ref() {
                Some(x) => x.clone(),
                None => return Err(TError::MissingField(String::from("FileSyncOutgoing.get_outgoing_files() -- FileSync record is missing `id` field. weird."))),
            };
            note_ids.push(note_id);
        }
        Ok(note_ids)
    }

    /// Remove a file sync record, usually once the file has uploaded
    /// successfully. Great success!
    fn delete_file_sync_record(&self, note_id: &String) -> TResult<()> {
        let mut file_sync: FileSync = Default::default();
        file_sync.id = Some(note_id.clone());
        with_db!{ db, self.db, "FileSyncOutgoing.delete_file_sync_record()",
            db.delete(&file_sync)?;
        }
        Ok(())
    }

    /// Returns a list of files that are waiting to be synced to the servers
    fn get_unsynced_files(&self) -> TResult<Vec<PathBuf>> {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        let user_id = match guard.user_id.as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("FileSyncOutgoing.get_unsynced_files() -- sync config `user_id` is None"))),
        };

        // grab the note ids for notes that have queued file uploads.
        let note_ids = self.get_outgoing_files()?;
        // for each note id, grab the associated outgoing file
        let mut files: Vec<PathBuf> = Vec::with_capacity(note_ids.len());
        for note_id in note_ids {
            let file = match FileData::file_finder(Some(user_id), Some(&note_id)) {
                Ok(x) => x,
                Err(e) => match e {
                    TError::NotFound(_) => { continue },
                    _ => return Err(e),
                },
            };
            files.push(file);
        }
        Ok(files)
    }

    /// Stream a local file up to our heroic API
    fn upload_file(&mut self, file: PathBuf) -> TResult<()> {
        let upload = |self, file| -> TResult<()> {
            let note_id: String = FileData::get_note_id(&file)?;

            // start our API call to the note file attachment endpoint
            let url = format!("/notes/{}/attachment", note_id);
            let req = ApiReq::new().header("Content-Type", &String::from("application/octet-stream"));
            // get an API stream we can start piping file data into
            let (mut stream, info) = self.api.call_start(api::Method::Put, &url[..], req)?;
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

            self.api.call_end::<Value>(stream.send(), info)?;
        };

        // if we're still here, the upload succeeded. remove the `file_sync`
        // record that tells us we should be uploading this file
        self.delete_file_sync_record(&note_id)?;
        // let the UI know how great we are. you will love this app. tremendous
        // app.
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
        let files = self.get_unsynced_files()?;
        for file in files {
            info!("FileSyncOutgoing.run_sync() -- syncing file {:?}", file);
            self.upload_file(file)?;
        }
        Ok(())
    }
}

