use ::std::sync::{Arc, RwLock};
use ::std::io::ErrorKind;
use ::jedi::{self, Value};
use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::{SyncModel, MemorySaver};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models;
use ::models::protected::Protected;
use ::models::model::Model;
use ::models::user::User;
use ::models::keychain::KeychainEntry;
use ::models::space::Space;
use ::models::invite::Invite;
use ::models::board::Board;
use ::models::note::Note;
use ::models::file::FileData;
use ::models::sync_record::{SyncType, SyncRecord, SyncAction};
use ::turtl::Turtl;
use ::std::mem;

const SYNC_IGNORE_KEY: &'static str = "sync:incoming:ignore";

/// Defines a struct for deserializing our incoming sync response
#[derive(Deserialize, Debug)]
struct SyncResponse {
    #[serde(default)]
    records: Vec<SyncRecord>,
    #[serde(default)]
    #[serde(deserialize_with = "::util::ser::str_i64_converter::deserialize")]
    sync_id: i64,
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
    match jedi::get_opt::<Vec<i64>>(&["sync_ids"], val_with_sync_ids) {
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
    pub fn ignore_on_next(db: &mut Storage, sync_ids: &Vec<i64>) -> TResult<()> {
        let mut ignored = SyncIncoming::get_ignored_impl(db)?;
        for sync_id in sync_ids {
            ignored.push(sync_id.to_string());
        }
        db.kv_set("sync:incoming:ignore", &jedi::stringify(&ignored)?)
    }

    /// Get all sync ids that should be ignored on the next sync run
    fn get_ignored(&self) -> TResult<Vec<String>> {
        with_db!{ db, self.db, SyncIncoming::get_ignored_impl(db) }
    }

    /// Clear out ignored sync ids
    fn clear_ignored(&self) -> TResult<()> {
        with_db!{ db, self.db, db.kv_delete(SYNC_IGNORE_KEY) }
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
            Err(e) => {
                let e = e.shed();
                match e {
                    TError::Io(io) => {
                        self.connected(false);
                        match io.kind() {
                            ErrorKind::TimedOut => return Ok(()),
                            _ => return TErr!(TError::Io(io)),
                        }
                    }
                    _ => return Err(e),
                }
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

        with_db!{ db, self.db,
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

        // send our incoming syncs into a queue that the Turtl/dispatch thread
        // can read and process. The purpose is to run MemorySaver for the syncs
        // which can only happen if we have access to Turtl, which we DO NOT
        // at this particular juncture.
        let sync_incoming_queue = {
            let conf = self.get_config();
            let sync_config_guard = conf.read().unwrap();
            sync_config_guard.incoming_sync.clone()
        };
        // queue em
        for rec in records { sync_incoming_queue.push(rec); }
        // this is what tells our dispatch thread to load the queued incoming
        // syncs and process them
        messaging::app_event("sync:incoming", &())?;

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
                info!("SyncIncoming::run_sync_item() -- got missing item, probably an add/delete: {:?}", sync_item);
                return Ok(());
            } else {
                return TErr!(TError::BadValue(format!("bad item: {:?}", sync_item)));
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
        let sync_id = with_db!{ db, self.db,
            db.kv_get("sync_id")?
        };
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
        let sync_id = with_db!{ db, self.db,
            db.kv_get("sync_id")?
        };
        let res = match sync_id {
            Some(ref x) => self.sync_from_api(x, true),
            None => return TErr!(TError::MissingData(String::from("no sync_id present"))),
        };
        res
    }
}

/// Grabs sync records off our Turtl.incoming_sync queue (sent to us from our
/// incoming sync thread). It's important to know that this function runs with
/// access to the Turtl data as one of the main dispatch threads, NOT in the
/// incoming sync thread.
///
/// Essentially, this is what's responsible for running MemorySaver for our
/// incoming syncs.
pub fn process_incoming_sync(turtl: &Turtl) -> TResult<()> {
    let sync_incoming_queue = {
        let sync_config_guard = turtl.sync_config.read().unwrap();
        sync_config_guard.incoming_sync.clone()
    };
    loop {
        let sync_incoming_lock = turtl.incoming_sync_lock.lock();
        println!("pis: locked");
        let sync_item = match sync_incoming_queue.try_pop() {
            Some(x) => x,
            None => break,
        };
        match sync_item.action.clone() {
            SyncAction::Add | SyncAction::Edit => {
                fn load_save<T>(turtl: &Turtl, mut sync_item: SyncRecord) -> TResult<()>
                    where T: Protected + MemorySaver
                    {
                        let mut data = Value::Null;
                        match sync_item.data.as_mut() {
                            Some(x) => mem::swap(&mut data, x),
                            None => return TErr!(TError::MissingData(format!("sync item missing `data` field."))),
                        }
                        let model: T = jedi::from_val(data)?;
                        model.save_to_mem(turtl)?;
                        messaging::ui_event("sync:incoming", &sync_item)?;
                        Ok(())
                    }
                match sync_item.ty.clone() {
                    SyncType::User => load_save::<User>(turtl, sync_item)?,
                    SyncType::Keychain => load_save::<KeychainEntry>(turtl, sync_item)?,
                    SyncType::Space => load_save::<Space>(turtl, sync_item)?,
                    SyncType::Board => load_save::<Board>(turtl, sync_item)?,
                    SyncType::Note => load_save::<Note>(turtl, sync_item)?,
                    SyncType::File => load_save::<FileData>(turtl, sync_item)?,
                    SyncType::Invite => load_save::<Invite>(turtl, sync_item)?,
                    _ => (),
                }
            }
            SyncAction::Delete => {
                fn load_delete<T>(turtl: &Turtl, sync_item: SyncRecord) -> TResult<()>
                    where T: Protected + MemorySaver
                    {
                        let mut model: T = Default::default();
                        model.set_id(sync_item.item_id.clone());
                        model.delete_from_mem(turtl)?;
                        messaging::ui_event("sync:incoming", &sync_item)?;
                        Ok(())
                    }
                match sync_item.ty.clone() {
                    SyncType::User => load_delete::<User>(turtl, sync_item)?,
                    SyncType::Keychain => load_delete::<KeychainEntry>(turtl, sync_item)?,
                    SyncType::Space => load_delete::<Space>(turtl, sync_item)?,
                    SyncType::Board => load_delete::<Board>(turtl, sync_item)?,
                    SyncType::Note => load_delete::<Note>(turtl, sync_item)?,
                    SyncType::File => load_delete::<FileData>(turtl, sync_item)?,
                    SyncType::Invite => load_delete::<Invite>(turtl, sync_item)?,
                    _ => (),
                }
            }
            _ => {}
        }
        drop(sync_incoming_lock);
    }
    Ok(())
}


