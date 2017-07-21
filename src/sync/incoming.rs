use ::std::sync::{Arc, RwLock};
use ::std::io::ErrorKind;

use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging::Messenger;
use ::models;
use ::models::sync_record::SyncRecord;

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
    db: Arc<Storage>,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    handlers: Handlers,
}

impl SyncIncoming {
    /// Create a new incoming syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<Storage>) -> SyncIncoming {
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
        let syncres = self.api.get(url.as_str(), ApiReq::new().timeout(timeout));
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
    fn update_local_db_from_api_sync(&self, syncdata: Value, force: bool) -> TResult<()> {
        // sometimes the sync call takes a while, and it's possible we've quit
        // mid-call. if this is the case, throw out our sync result.
        if self.should_quit() && !force { return Ok(()); }
        // same, but with enabled
        if !self.is_enabled() && !force { return Ok(()); }
        // if our sync data is blank, then alright, forget it. YOU KNOW WHAT?
        // HEY JUST FORGET IT!
        if syncdata == Value::Null { return Ok(()); }

        // the api sends back the latest sync id out of the bunch. grab it.
        let sync_id = (jedi::get::<u64>(&["sync_id"], &syncdata)?).to_string();
        // also grab our sync records.
        let records: Vec<SyncRecord> = jedi::get(&["records"], &syncdata)?;

        // start a transaction. we don't want to save half-data.
        self.db.conn.execute("BEGIN TRANSACTION", &[])?;
        for rec in records {
            self.run_sync_item(rec)?;
        }
        // make sure we save our sync_id as the LAST STEP of our transaction.
        // if this fails, then next time we load we just start from the same
        // spot we were at before. SYNC BUGS HATE HIM!!!1
        self.db.kv_set("sync_id", &sync_id)?;
        // ok, commit
        self.db.conn.execute("COMMIT TRANSACTION", &[])?;
        Ok(())
    }

    /// Sync an individual incoming sync item to our DB.
    fn run_sync_item(&self, sync_item: SyncRecord) -> TResult<()> {
        // check if we have missing data, and if so, if it's on purpose
        if sync_item.data.is_none() {
            let missing = match sync_item.missing {
                Some(x) => x,
                None => false,
            };
            if missing {
                info!("sync::incoming::run_sync_item() -- got missing item, probably and add/delete: {:?}", sync_item);
                return Ok(());
            } else {
                return Err(TError::BadValue(format!("sync::incoming::run_sync_item() -- bad item: {:?}", sync_item)));
            }
        }

        // send our sync item off to each type's respective handler. these are
        // defined by the SyncModel (sync/sync_model.rs).
        match sync_item.ty.as_ref() {
            "user" => self.handlers.user.incoming(&self.db, sync_item),
            "keychain" => self.handlers.keychain.incoming(&self.db, sync_item),
            "space" => self.handlers.space.incoming(&self.db, sync_item),
            "board" => self.handlers.board.incoming(&self.db, sync_item),
            "note" => self.handlers.note.incoming(&self.db, sync_item),
            "file" => self.handlers.file.incoming(&self.db, sync_item),
            "invite" => self.handlers.invite.incoming(&self.db, sync_item),
            _ => return Err(TError::BadValue(format!("SyncIncoming.run_sync_item() -- unknown sync type encountered: {}", sync_item.ty))),
        }
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
        let sync_id = self.db.kv_get("sync_id")?;
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

    fn run_sync(&self) -> TResult<()> {
        let sync_id = self.db.kv_get("sync_id")?;
        let res = match sync_id {
            Some(ref x) => self.sync_from_api(x, true),
            None => return Err(TError::MissingData(String::from("SyncIncoming.run_sync() -- no sync_id present"))),
        };
        res
    }
}


