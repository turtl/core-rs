use ::std::sync::{Arc, RwLock};
use ::std::io::ErrorKind;

use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging::{self, Messenger};
use ::models;
use ::models::sync_record::{SyncType, SyncRecord};

/// Defines a struct for deserializing our incoming sync response
#[derive(Deserialize, Debug)]
struct SyncResponse {
    #[serde(default)]
    records: Vec<SyncRecord>,
    #[serde(default)]
    sync_id: u64,
}

struct Handlers {
    user: models::user::User,
    keychain: models::keychain::KeychainEntry,
    space: models::space::Space,
    board: models::board::Board,
    note: models::note::Note,
    file: models::file::FileData,
    invite: models::invite::Invite,
}

/// Holds the state for data going from API -> turtl (incoming sync data),
/// including tracking which sync item's we've seen and which we haven't.
pub struct SyncIncoming {
    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data (such
    /// as our last sync_id).
    db: Arc<RwLock<Option<Storage>>>,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    handlers: Handlers,
}

impl SyncIncoming {
    /// Create a new incoming syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> SyncIncoming {
        let handlers = Handlers {
            user: models::user::User::new(),
            keychain: models::keychain::KeychainEntry::new(),
            space: models::space::Space::new(),
            board: models::board::Board::new(),
            note: models::note::Note::new(),
            file: models::file::FileData::new(),
            invite: models::invite::Invite::new(),
        };

        SyncIncoming {
            config: config,
            api: api,
            db: db,
            handlers: handlers,
        }
    }

    /// Grab the latest changes from the API (anything after the given sync ID).
    /// Also, if `poll` is true, we long-poll.
    fn sync_from_api(&self, sync_id: &String, poll: bool) -> TResult<()> {
        let immediate = if poll { "0" } else { "1" };
        let url = format!("/sync?sync_id={}&immediate={}", sync_id, immediate);
        let timeout = if poll { 60 } else { 10 };
        let syncres: TResult<SyncResponse> = self.api.get(url.as_str(), ApiReq::new().timeout(timeout));
        // if we have a timeout just return Ok(()) (the sync system is built to
        // timeout if no response is received)
        let syncdata = match syncres {
            Ok(x) => x,
            Err(e) => match e {
                TError::Io(io) => {
                    self.connected(false);
                    match io.kind() {
                        ErrorKind::TimedOut => return Ok(()),
                        _ => return Err(TError::Io(io)),
                    }
                }
                _ => return Err(e),
            },
        };

        self.connected(true);
        self.update_local_db_from_api_sync(syncdata, !poll)
    }

    /// Load the user's entire profile. The API gives us back a set of sync
    /// objects, which is super handy because we can just treat them like any
    /// other sync
    fn load_full_profile(&self) -> TResult<()> {
        let syncdata = self.api.get("/sync/full", ApiReq::new().timeout(120))?;
        self.connected(true);
        self.update_local_db_from_api_sync(syncdata, true)
    }

    /// Take sync data we got from the API and update our local database with
    /// it. Kewl.
    fn update_local_db_from_api_sync(&self, syncdata: SyncResponse, force: bool) -> TResult<()> {
        // sometimes the sync call takes a while, and it's possible we've quit
        // mid-call. if this is the case, throw out our sync result.
        if self.should_quit() && !force { return Ok(()); }
        // same, but with enabled
        if !self.is_enabled() && !force { return Ok(()); }

        // destructure our response
        let SyncResponse { sync_id, records } = syncdata;

        // split our sync records up. file syncs will go into the `sync` table
        // with a type of SyncType::FileIncoming, everyone else gets handled
        // immediately.
        let mut item_syncs = Vec::with_capacity(records.len());
        let mut file_syncs = Vec::with_capacity(4);
        for sync in records {
            // NOTE: it's unlikely/impossible that we'd EVER have the type
            // FileIncoming here, but i like to be cautious.
            if sync.ty == SyncType::File || sync.ty == SyncType::FileIncoming {
                file_syncs.push(sync);
            } else {
                item_syncs.push(sync);
            }
        }

        with_db!{ db, self.db, "SyncIncoming.update_local_db_from_api_sync()",
            // start a transaction. running incoming sync is all or nothing.
            db.conn.execute("BEGIN TRANSACTION", &[])?;

            // move our file sync records into the `sync` table
            for mut sync in file_syncs {
                sync.ty = SyncType::FileIncoming;
                sync.db_save(db)?;
            }

            // run each of our non-file syncs locally. this generally means
            // saving the contained item to our local DB and notifying the UI
            // we have a changed item (so it can reload if needed)
            for rec in item_syncs {
                self.run_sync_item(db, rec)?;
            }

            // make sure we save our sync_id as the LAST STEP of our transaction.
            // if this fails, then next time we load we just start from the same
            // spot we were at before. SYNC BUGS HATE HIM!!!1
            db.kv_set("sync_id", &sync_id.to_string())?;

            // ok, commit
            db.conn.execute("COMMIT TRANSACTION", &[])?;
        }
        Ok(())
    }

    /// Sync an individual incoming sync item to our DB.
    fn run_sync_item(&self, db: &mut Storage, sync_item: SyncRecord) -> TResult<()> {
        // check if we have missing data, and if so, if it's on purpose
        if sync_item.data.is_none() {
            let missing = match sync_item.missing {
                Some(x) => x,
                None => false,
            };
            if missing {
                info!("sync::incoming::run_sync_item() -- got missing item, probably an add/delete: {:?}", sync_item);
                return Ok(());
            } else {
                return Err(TError::BadValue(format!("sync::incoming::run_sync_item() -- bad item: {:?}", sync_item)));
            }
        }

        // send our sync item off to each type's respective handler. these are
        // defined by the SyncModel (sync/sync_model.rs).
        let sync_clone = sync_item.clone_shallow();
        match sync_item.ty {
            SyncType::User => self.handlers.user.incoming(db, sync_item),
            SyncType::Keychain => self.handlers.keychain.incoming(db, sync_item),
            SyncType::Space => self.handlers.space.incoming(db, sync_item),
            SyncType::Board => self.handlers.board.incoming(db, sync_item),
            SyncType::Note => self.handlers.note.incoming(db, sync_item),
            SyncType::File => self.handlers.file.incoming(db, sync_item),
            SyncType::Invite => self.handlers.invite.incoming(db, sync_item),
            _ => {
                error!("SyncIncoming.run_sync_item() -- sync type {:?} is not implemented", sync_item.ty);
                return Err(TError::NotImplemented);
            },
        }?;

        // let the ui know we got a sync!
        messaging::ui_event("sync:incoming", &sync_clone)?;
        Ok(())
    }
}

impl Syncer for SyncIncoming {
    fn get_name(&self) -> &'static str {
        "incoming"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn init(&self) -> TResult<()> {
        let sync_id = with_db!{ db, self.db, "SyncIncoming.init()",
            db.kv_get("sync_id")?
        };
        Messenger::event("sync:incoming:init:start", jedi::obj())?;
        let skip_init = {
            let config_guard = self.config.read().unwrap();
            config_guard.skip_api_init
        };
        let res = if !skip_init {
            match sync_id {
                // we have a sync id! grab the latest changes from the API
                Some(ref x) => self.sync_from_api(x, false),
                // no sync id ='[ ='[ ='[ ...instead grab the full profile
                None => self.load_full_profile(),
            }
        } else {
            Ok(())
        };
        res
    }

    fn run_sync(&mut self) -> TResult<()> {
        let sync_id = with_db!{ db, self.db, "SyncIncoming.run_sync()",
            db.kv_get("sync_id")?
        };
        let res = match sync_id {
            Some(ref x) => self.sync_from_api(x, true),
            None => return Err(TError::MissingData(String::from("SyncIncoming.run_sync() -- no sync_id present"))),
        };
        res
    }
}


