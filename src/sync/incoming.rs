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
use ::models::model::Model;
use ::models::sync_record::{SyncType, SyncRecord};
use ::turtl::Turtl;

const SYNC_IGNORE_KEY: &'static str = "sync:incoming:ignore";

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

/// Given a Value object with sync_ids, try to ignore the sync ids. Kids' stuff.
pub fn ignore_syncs_maybe(turtl: &Turtl, val_with_sync_ids: &Value, errtype: &str) {
    match jedi::get_opt::<Vec<u64>>(&["sync_ids"], val_with_sync_ids) {
        Some(x) => {
            let mut db_guard = turtl.db.write().unwrap();
            if db_guard.is_some() {
                match SyncIncoming::ignore_on_next(db_guard.as_mut().unwrap(), &x) {
                    Ok(..) => {},
                    Err(e) => warn!("{} -- error ignoring sync items: {}", errtype, e),
                }
            }
        }
        None => {}
    }
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

    /// Get all sync ids that should be ignored on the next sync run
    fn get_ignored_impl(db: &mut Storage) -> TResult<Vec<String>> {
        let ignored = match db.kv_get(SYNC_IGNORE_KEY)? {
            Some(x) => jedi::parse(&x)?,
            None => Vec::new(),
        };
        Ok(ignored)
    }

    /// Static handler for ignoring sync items.
    ///
    /// Tracks which sync items we should ignore on incoming. The idea is that
    /// when an outgoing sync creates a note, that note will already be created
    /// on our side (client side). So the outgoing sync adds the sync ids of the
    /// syncs created while adding that note to the ignore list, and when the
    /// incoming sync returns, it won't re-run the syncs for the items we are
    /// already up-to-date on locally. Note that this is not strictly necessary
    /// since the sync system should be able to handle situations like this
    /// (such as double-adding a note), however it can be much more efficient
    /// especially in the case of file syncing.
    pub fn ignore_on_next(db: &mut Storage, sync_ids: &Vec<u64>) -> TResult<()> {
        let mut ignored = SyncIncoming::get_ignored_impl(db)?;
        for sync_id in sync_ids {
            ignored.push(sync_id.to_string());
        }
        db.kv_set("sync:incoming:ignore", &jedi::stringify(&ignored)?)
    }

    /// Get all sync ids that should be ignored on the next sync run
    fn get_ignored(&self) -> TResult<Vec<String>> {
        with_db!{ db, self.db, "SyncIncoming.get_ignored()", SyncIncoming::get_ignored_impl(db) }
    }

    /// Clear out ignored sync ids
    fn clear_ignored(&self) -> TResult<()> {
        with_db!{ db, self.db, "SyncIncoming.clear_ignored()", db.kv_delete(SYNC_IGNORE_KEY) }
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

        // grab sync ids we're ignoring
        let ignored = self.get_ignored()?;
        // filter out ignored records
        let mut records = records
            .into_iter()
            .filter(|rec| {
                match rec.id() {
                    Some(id) => {
                        if ignored.contains(id) {
                            debug!("SyncIncoming.update_local_db_from_api_sync() -- ignoring {}", id);
                            false
                        } else {
                            true
                        }
                    }
                    None => { true }
                }
            })
            .collect::<Vec<_>>();

        with_db!{ db, self.db, "SyncIncoming.update_local_db_from_api_sync()",
            // start a transaction. running incoming sync is all or nothing.
            db.conn.execute("BEGIN TRANSACTION", &[])?;
            for rec in &mut records {
                self.run_sync_item(db, rec)?;
            }
            // make sure we save our sync_id as the LAST STEP of our transaction.
            // if this fails, then next time we load we just start from the same
            // spot we were at before. SYNC BUGS HATE HIM!!!1
            db.kv_set("sync_id", &sync_id.to_string())?;
            // ok, commit
            db.conn.execute("COMMIT TRANSACTION", &[])?;
        }

        for sync_record in records {
            // let the app know we have an incoming sync. the purpose of this is
            // mainly to run MemorySaver::save_to_mem/delete_from_mem on the
            // model that gots synced. since those require access to the Turtl
            // object, and we don't have access here, we need to pass a message
            // to the dispatch to tell it to run the mem saver on this item.
            //
            // NOTE: we don't want to run this inside of run_sync_item for two
            // reasons:
            //  - if ANY of the above items fails to sync, the entire
            //    transaction is rolled back. this means it's possible that we
            //    would have called memsaver on an item that didn't actually end
            //    up changing (SO embarassing...)
            //  - the DB is locked during the entire loop above, and if the
            //    memsaver uses the DB at all it will just block until the loop
            //    is over.
            messaging::app_event("sync:incoming", &sync_record)?;
            // let the ui know we got a sync!
            messaging::ui_event("sync:incoming", &sync_record)?;
        }

        // clear out the sync ignore list
        match self.clear_ignored() {
            Ok(_) => {},
            Err(e) => error!("SyncIncoming.update_local_db_from_api_sync() -- error clearing out ignored syncs (but continue because it's not really a big deal): {}", e),
        }

        Ok(())
    }

    /// Sync an individual incoming sync item to our DB.
    fn run_sync_item(&self, db: &mut Storage, sync_item: &mut SyncRecord) -> TResult<()> {
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
        match sync_item.ty {
            SyncType::User => self.handlers.user.incoming(db, sync_item),
            SyncType::Keychain => self.handlers.keychain.incoming(db, sync_item),
            SyncType::Space => self.handlers.space.incoming(db, sync_item),
            SyncType::Board => self.handlers.board.incoming(db, sync_item),
            SyncType::Note => self.handlers.note.incoming(db, sync_item),
            SyncType::File | SyncType::FileIncoming => self.handlers.file.incoming(db, sync_item),
            SyncType::Invite => self.handlers.invite.incoming(db, sync_item),
        }?;

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


