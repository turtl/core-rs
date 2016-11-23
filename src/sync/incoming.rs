use ::std::sync::{Arc, RwLock};
use ::std::io::ErrorKind;

use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::sync::sync_model::SyncModel;
use ::util::thredder::Pipeline;
use ::util::event::Emitter;
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging::Messenger;
use ::models;

struct Handlers {
    user: models::user::User,
    keychain: models::keychain::KeychainEntry,
    persona: models::persona::Persona,
    board: models::board::Board,
    note: models::note::Note,
    file: models::file::FileData,
    invite: models::invite::Invite,
}

/// Holds the state for data going from API -> turtl (incoming sync data),
/// including tracking which sync item's we've seen and which we haven't.
pub struct SyncIncoming {
    /// The message channel to our main thread.
    tx_main: Pipeline,

    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data (such
    /// as our lsat sync_id).
    db: Arc<Storage>,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    handlers: Handlers,
}

impl SyncIncoming {
    /// Create a new incoming syncer
    pub fn new(tx_main: Pipeline, config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<Storage>) -> SyncIncoming {
        let handlers = Handlers {
            user: models::user::User::new(),
            keychain: models::keychain::KeychainEntry::new(),
            persona: models::persona::Persona::new(),
            board: models::board::Board::new(),
            note: models::note::Note::new(),
            file: models::file::FileData::new(),
            invite: models::invite::Invite::new(),
        };

        SyncIncoming {
            tx_main: tx_main,
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
        self.update_local_db_from_api_sync(syncdata)
    }

    /// Load the user's entire profile. The API gives us back a set of sync
    /// objects, which is super handy because we can just treat them like any
    /// other sync
    fn load_full_profile(&self) -> TResult<()> {
        let syncdata = self.api.get("/sync/full", ApiReq::new())?;
        self.connected(true);
        self.update_local_db_from_api_sync(syncdata)
    }

    /// Take sync data we got from the API and update our local database with
    /// it. Kewl.
    fn update_local_db_from_api_sync(&self, syncdata: Value) -> TResult<()> {
        // sometimes the sync call takes a while, and it's possible we've quit
        // mid-call. if this is the case, throw out our sync result.
        if self.should_quit() { return Ok(()); }
        // same, but with enabled
        if !self.is_enabled() { return Ok(()); }
        // if our sync data is blank, then alright, forget it. YOU KNOW WHAT?
        // HEY JUST FORGET IT!
        if syncdata == Value::Null { return Ok(()); }

        // the api sends back the latest sync id out of the bunch. grab it.
        let sync_id = jedi::get::<String>(&["sync_id"], &syncdata)?;
        // also grab our sync records.
        let records = jedi::get::<Vec<Value>>(&["records"], &syncdata)?;

        // start a transaction. we don't want to save half-data.
        self.db.conn.execute("BEGIN TRANSACTION", &[])?;
        for rec in records {
            self.run_sync_item(rec)?
        }
        // make sure we save our sync_id as the LAST STEP of our transaction.
        // if this fails, then next time we load we just start from the same
        // spot we were at before
        self.db.kv_set("sync_id", &sync_id)?;
        // ok, commit
        self.db.conn.execute("COMMIT TRANSACTION", &[])?;
        Ok(())
    }

    /// Sync an individual incoming sync item to our DB.
    fn run_sync_item(&self, data: Value) -> TResult<()> {
        let sync_type = jedi::get::<String>(&["type"], &data)?;
        let res = match sync_type.as_ref() {
            "user" => self.handlers.user.incoming(&self.db, data),
            "keychain" => self.handlers.keychain.incoming(&self.db, data),
            "persona" => self.handlers.persona.incoming(&self.db, data),
            "board" => self.handlers.board.incoming(&self.db, data),
            "note" => self.handlers.note.incoming(&self.db, data),
            "file" => self.handlers.file.incoming(&self.db, data),
            "invite" => self.handlers.invite.incoming(&self.db, data),
            _ => return Err(TError::BadValue(format!("SyncIncoming.run_sync_item() -- unknown sync type encountered: {}", sync_type))),
        };
        res
    }
}

impl Syncer for SyncIncoming {
    fn get_name(&self) -> &'static str {
        "incoming"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn get_tx(&self) -> Pipeline {
        self.tx_main.clone()
    }

    fn init(&self) -> TResult<()> {
        let sync_id = self.db.kv_get("sync_id")?;
        Messenger::event(String::from("sync:incoming:init:start").as_str(), jedi::obj())?;
        let res = match sync_id {
            // we have a sync id! grab the latest changes from the API
            Some(ref x) => self.sync_from_api(x, false),
            // no sync id ='[ ='[ ='[ ...instead grab the full profile
            None => self.load_full_profile(),
        };
        // let our Turtl know we're done
        self.get_tx().next(|turtl| {
            turtl.events.trigger("sync:incoming:init:done", &jedi::obj());
        });
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


