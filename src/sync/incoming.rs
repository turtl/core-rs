use ::std::sync::{Arc, RwLock, Mutex};
use ::std::io::ErrorKind;
use ::jedi::{self, Value};
use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::{SyncModel, MemorySaver};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models;
use ::models::protected::{Protected, Keyfinder};
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
use ::config;
use ::util;

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

/// Lets the server know why we are asking for an incoming sync.
#[derive(Debug, Serialize, PartialEq)]
enum SyncReason {
    #[serde(rename = "poll")]
    Poll,
    #[serde(rename = "reconnect")]
    Reconnect,
    #[serde(rename = "initial")]
    Initial,
}

/// Given a Value object with sync_ids, try to ignore the sync ids. Kids' stuff.
pub fn ignore_syncs_maybe(turtl: &Turtl, val_with_sync_ids: &Value, errtype: &str) {
    match jedi::get_opt::<Vec<i64>>(&["sync_ids"], val_with_sync_ids) {
        Some(x) => {
            let mut db_guard = lock!(turtl.db);
            if db_guard.is_some() {
                match SyncIncoming::ignore_on_next(db_guard.as_mut().expect("turtl::sync_incoming::ignore_syncs_maybe() -- db is None"), &x) {
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
    db: Arc<Mutex<Option<Storage>>>,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    handlers: Handlers,

    /// Stores whether or not we're connected. Used internally, mainly to
    /// determine whether we should check sync immediate (if disconnected) or
    /// long-poll (if connected).
    connected: bool,

    /// Stores our syn run version
    run_version: i64,
}

impl SyncIncoming {
    /// Create a new incoming syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<Mutex<Option<Storage>>>) -> SyncIncoming {
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
            connected: false,
            run_version: 0,
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
        db.kv_set(SYNC_IGNORE_KEY, &jedi::stringify(&ignored)?)
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
    fn sync_from_api(&mut self, sync_id: &String, reason: SyncReason) -> TResult<()> {
        let reason_s = util::enum_to_string(&reason)?;
        let url = format!("/sync?sync_id={}&type={}", sync_id, reason_s);
        let timeout = match &reason {
            SyncReason::Poll => {
                config::get(&["sync", "poll_timeout"]).unwrap_or(60)
            }
            _ => 10
        };
        let syncres: TResult<SyncResponse> = self.api.get(url.as_str(), ApiReq::new().timeout(timeout));

        // ^ this call can take a while. if sync got disabled while it was
        // taking its sweet time, then bail on the result.
        if !self.is_enabled() { return Ok(()); }

        // if we have a timeout just return Ok(()) (the sync system is built to
        // timeout if no response is received)
        let syncdata = match syncres {
            Ok(x) => x,
            Err(e) => {
                let e = e.shed();
                match e {
                    TError::Io(io) => {
                        match io.kind() {
                            ErrorKind::TimedOut => return Ok(()),
                            // android throws this at us quite often, would
                            // be nice to know why, but for now going to just
                            // ignore it.
                            ErrorKind::WouldBlock => return Ok(()),
                            _ => {
                                info!("SyncIncoming.sync_from_api() -- unknown IO error kind: {:?}", io.kind());
                                self.set_connected(false);
                                return TErr!(TError::Io(io));
                            }
                        }
                    }
                    TError::Api(status, msg) => {
                        self.set_connected(false);
                        return TErr!(TError::Api(status, msg));
                    }
                    _ => return Err(e),
                }
            },
        };

        self.set_connected(true);
        self.update_local_db_from_api_sync(syncdata, reason != SyncReason::Poll)
    }

    /// Load the user's entire profile. The API gives us back a set of sync
    /// objects, which is super handy because we can just treat them like any
    /// other sync
    fn load_full_profile(&mut self) -> TResult<()> {
        let syncdata = self.api.get("/sync/full", ApiReq::new().timeout(120))?;
        self.set_connected(true);
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
        let mut ignore_count = 0;
        // filter out ignored records
        let mut records = records
            .into_iter()
            .filter(|rec| {
                match rec.id() {
                    Some(id) => {
                        if ignored.contains(id) {
                            debug!("SyncIncoming.update_local_db_from_api_sync() -- ignoring {}", id);
                            ignore_count += 1;
                            false
                        } else {
                            true
                        }
                    }
                    None => { true }
                }
            })
            .collect::<Vec<_>>();

        info!("SyncIncoming.update_local_db_from_api_sync() -- ignored {} incoming syncs", ignore_count);
        with_db!{ db, self.db,
            // start a transaction. running incoming sync is all or nothing.
            db.conn.execute("BEGIN TRANSACTION", &[])?;
            for rec in &mut records {
                self.run_sync_item(db, rec)?;
            }
            // save our sync id
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
            let sync_config_guard = lockr!(conf);
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
            SyncType::FileOutgoing => Ok(()),
        }?;

        Ok(())
    }

    fn set_connected(&mut self, yesno: bool) {
        self.connected = yesno;
        self.connected(yesno);
    }
}

impl Syncer for SyncIncoming {
    fn get_name(&self) -> &'static str {
        "incoming"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn set_run_version(&mut self, run_version: i64) {
        self.run_version = run_version;
    }

    fn get_run_version(&self) -> i64 {
        self.run_version
    }

    fn init(&mut self) -> TResult<()> {
        let sync_id = with_db!{ db, self.db, db.kv_get("sync_id") }?;
        let skip_init = {
            let config_guard = lockr!(self.config);
            config_guard.skip_api_init
        };
        let res = if !skip_init {
            match sync_id {
                // we have a sync id! grab the latest changes from the API
                Some(ref x) => self.sync_from_api(x, SyncReason::Initial),
                // no sync id ='[ ='[ ='[ ...instead grab the full profile
                None => self.load_full_profile(),
            }
        } else {
            Ok(())
        };
        res
    }

    fn run_sync(&mut self) -> TResult<()> {
        let sync_id = with_db!{ db, self.db, db.kv_get("sync_id") }?;
        // note that when syncing changes from the server, we only poll if we
        // are currently connected. this way, if we DO get a connection back
        // after being previously disconnected, we can update our state
        // immediately instead of waiting 60s or w/e until the sync goes through
        let reason = if self.connected { SyncReason::Poll } else { SyncReason::Reconnect };
        let res = match sync_id {
            Some(ref x) => self.sync_from_api(x, reason),
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
        let sync_config_guard = lockr!(turtl.sync_config);
        sync_config_guard.incoming_sync.clone()
    };
    loop {
        let sync_incoming_lock = turtl.incoming_sync_lock.lock();
        let sync_item = match sync_incoming_queue.try_pop() {
            Some(x) => x,
            None => break,
        };
        fn mem_save<T>(turtl: &Turtl, mut sync_item: SyncRecord) -> TResult<()>
            where T: Protected + MemorySaver + Keyfinder
        {
            let model = if &sync_item.action == &SyncAction::Delete {
                let mut model: T = Default::default();
                model.set_id(sync_item.item_id.clone());
                model
            } else {
                let mut data = Value::Null;
                match sync_item.data.as_mut() {
                    Some(x) => mem::swap(&mut data, x),
                    None => return TErr!(TError::MissingData(format!("sync item missing `data` field."))),
                }
                let mut model: T = jedi::from_val(data)?;
                if model.should_deserialize_on_mem_update() {
                    turtl.find_model_key(&mut model)?;
                    model.deserialize()?;
                }
                model
            };
            model.run_mem_update(turtl, sync_item.action.clone())?;
            Ok(())
        }
        match sync_item.ty.clone() {
            SyncType::User => mem_save::<User>(turtl, sync_item)?,
            SyncType::Keychain => mem_save::<KeychainEntry>(turtl, sync_item)?,
            SyncType::Space => mem_save::<Space>(turtl, sync_item)?,
            SyncType::Board => mem_save::<Board>(turtl, sync_item)?,
            SyncType::Note => mem_save::<Note>(turtl, sync_item)?,
            SyncType::File => mem_save::<FileData>(turtl, sync_item)?,
            SyncType::Invite => mem_save::<Invite>(turtl, sync_item)?,
            _ => (),
        }
        drop(sync_incoming_lock);
    }
    Ok(())
}


