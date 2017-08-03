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

    /// Stores (in-mem) state of what files are being uploaded.
    processing: HashMap<String, bool>,
}

impl FileSyncOutgoing {
    /// Create a new outgoing syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> Self {
        FileSyncOutgoing {
            config: config,
            api: api,
            db: db,
            processing: HashMap::new(),
        }
    }

    /// Returns a list of note_ids for notes that have pending file uploads
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

    /// Returns a list of files that are wating to be synced to the servers
    fn get_unsynced_files(&self) -> TResult<Vec<PathBuf>> {
        let local_config = self.get_config();
        let guard = local_config.read().unwrap();
        let user_id = match guard.user_id.as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("FileSyncOutgoing.get_unsynced_files() -- sync config `user_id` is None"))),
        };

        let note_ids = self.get_outgoing_files()?;
        let note_ids = note_ids.into_iter()
            .filter(|x| !self.processing.contains_key(x))
            .collect::<Vec<String>>();

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
        let note_id: String = FileData::get_note_id(&file)?;
        self.processing.insert(note_id.clone(), true);
        let url = format!("/notes/{}/attachment", note_id);
        let req = ApiReq::new().header("Content-Type", &String::from("application/octet-stream"));
        let (mut stream, info) = self.api.call_start(api::Method::Put, &url[..], req)?;
        let mut file = fs::File::open(&file)?;
        let mut buf = [0; 4096];
        loop {
            let read = file.read(&mut buf[..])?;
            if read <= 0 { break; }
            let (read_bytes, _) = buf.split_at(read);
            let written = stream.write(read_bytes)?;
            if read != written {
                return Err(TError::Msg(format!("FileSyncOutgoing.upload_file() -- problem uploading file: grabbed {} bytes, only sent {}", read, written)));
            }
        }
        stream.flush()?;
        self.api.call_end::<Value>(stream.send(), info)?;
        self.delete_file_sync_record(&note_id)?;
        self.processing.remove(&note_id);
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

