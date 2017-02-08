//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info, and is passed
//! around to various pieces of the app running in the main thread.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;
use ::std::collections::HashMap;
use ::std::fs;

use ::regex::Regex;
use ::futures::Future;
use ::num_cpus;

use ::jedi::{self, Value};
use ::config;

use ::error::{TResult, TFutureResult, TError};
use ::crypto::Key;
use ::util;
use ::util::event::{self, Emitter};
use ::storage::{self, Storage};
use ::api::Api;
use ::profile::Profile;
use ::models::protected::{self, Keyfinder, Protected};
use ::models::model::Model;
use ::models::user::User;
use ::models::space::Space;
use ::models::board::Board;
use ::models::keychain::{self, KeyRef, KeychainEntry};
use ::models::note::Note;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::{Messenger, Response};
use ::sync::{self, SyncConfig, SyncState};
use ::search::Search;

/// Defines a container for our app's state. Note that most operations the user
/// has access to via messaging get this object passed to them.
pub struct Turtl {
    /// Our phone channel to the main thread. Although not generally used
    /// directly by the Turtl object, Turtl may spawn other processes that need
    /// it (eg after login the sync system needs it) to it's handy to have a
    /// copy laying around.
    pub tx_main: Pipeline,
    /// This is our app-wide event bus.
    pub events: event::EventEmitter,
    /// Holds our current user (Turtl only allows one logged-in user at once)
    pub user: RwLock<User>,
    /// Holds the user's data profile (keychain, boards, notes, etc, etc, etc)
    pub profile: RwLock<Profile>,
    /// Need to do some CPU-intensive work and have a Future finished when it's
    /// done? Send it here! Great for decrypting models.
    pub work: Thredder,
    /// Need to do some I/O and have a Future finished when it's done? Send it
    /// here! Great for API calls.
    pub async: Thredder,
    /// Allows us to send messages to our UI
    pub msg: Messenger,
    /// A storage system dedicated to key-value data. This *must* be initialized
    /// before our main local db because our local db is baed off the currently
    /// logged-in user, and we need persistant key-value storage even when
    /// logged out.
    pub kv: Arc<RwLock<Storage>>,
    /// Our main database, initialized after a successful login. This db is
    /// named via a function of the user ID and the server we're talking to,
    /// meaning we can have multiple databases that store different things for
    /// different people depending on server/user.
    pub db: RwLock<Option<Storage>>,
    /// Our external API object. Note that most things API-related go through
    /// the Sync system, but there are a handful of operations that Sync doesn't
    /// handle that need API access (invites come to mind). Use sparingly.
    pub api: Arc<Api>,
    /// Holds our heroic search object, used to index/find our notes once the
    /// profile is loaded.
    pub search: RwLock<Option<Search>>,
    /// Sync system configuration (shared state with the sync system).
    pub sync_config: Arc<RwLock<SyncConfig>>,
    /// Holds our sync state data
    sync_state: Arc<RwLock<Option<SyncState>>>,
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<Turtl>;

impl Turtl {
    /// Create a new Turtl app
    fn new(tx_main: Pipeline) -> TResult<Turtl> {
        let num_workers = num_cpus::get() - 1;

        let api = Arc::new(Api::new());
        let kv = Arc::new(RwLock::new(Turtl::open_kv()?));

        // make sure we have a client id
        storage::setup_client_id(kv.clone())?;

        let turtl = Turtl {
            tx_main: tx_main.clone(),
            events: event::EventEmitter::new(),
            user: RwLock::new(User::new()),
            profile: RwLock::new(Profile::new()),
            api: api,
            msg: Messenger::new(),
            work: Thredder::new("work", num_workers as u32),
            async: Thredder::new("async", 2),
            kv: kv,
            db: RwLock::new(None),
            search: RwLock::new(None),
            sync_config: Arc::new(RwLock::new(SyncConfig::new())),
            sync_state: Arc::new(RwLock::new(None)),
        };
        Ok(turtl)
    }

    /// A handy wrapper for creating a wrapped Turtl object (TurtlWrap),
    /// shareable across threads.
    pub fn new_wrap(tx_main: Pipeline) -> TResult<TurtlWrap> {
        let turtl = Arc::new(Turtl::new(tx_main)?);
        Ok(turtl)
    }

    /// Create/open a new KV store connection
    pub fn open_kv() -> TResult<Storage> {
        let data_folder = config::get::<String>(&["data_folder"])?;
        let kv_location = if data_folder == ":memory:" {
            String::from(":memory:")
        } else {
            format!("{}/turtl-kv.sqlite", &data_folder)
        };
        Ok(Storage::new(&kv_location, jedi::obj())?)
    }

    /// Send a message to (presumably) our UI.
    pub fn remote_send(&self, id: Option<String>, msg: String) -> TResult<()> {
        match id {
            Some(id) => self.msg.send_suffix(id, msg),
            None => self.msg.send(msg),
        }
    }

    /// Send a success response to a remote request
    pub fn msg_success(&self, mid: &String, data: Value) -> TResult<()> {
        let res = Response {
            e: 0,
            d: data,
        };
        let msg = jedi::stringify(&res)?;
        self.remote_send(Some(mid.clone()), msg)
    }

    /// Send an error response to a remote request
    pub fn msg_error(&self, mid: &String, err: &TError) -> TResult<()> {
        let res = Response {
            e: 1,
            d: Value::String(format!("{}", err)),
        };
        let msg = jedi::stringify(&res)?;
        self.remote_send(Some(mid.clone()), msg)
    }

    /// If an error occurs out of band of a request, send an error event
    pub fn error_event(&self, err: &TError, context: &str) -> TResult<()> {
        let val = Value::Array(vec![Value::String(String::from(context)), Value::String(format!("{}", err))]);
        Messenger::event("error", val)
    }

    /// Log a user in
    pub fn login(&self, username: String, password: String) -> TFutureResult<()> {
        self.with_next_fut()
            .and_then(move |turtl| -> TFutureResult<()> {
                let turtl2 = turtl.clone();
                User::login(turtl.clone(), &username, &password)
                    .and_then(move |_| -> TFutureResult<()> {
                        let db = ftry!(turtl2.create_user_db());
                        let mut db_guard = turtl2.db.write().unwrap();
                        *db_guard = Some(db);
                        drop(db_guard);
                        FOk!(())
                    })
                    .boxed()
            })
            .boxed()
    }

    /*
    pub fn join(&self, username: String, password: String) -> TFutureResult<()> {
        self.with_next_fut()
            .and_then(move |turtl| -> TFutureResult<()> {
                let turtl2 = turtl.clone();
                User::join(turtl.clone(), &username, &password)
                    .and_then(move |_| -> TFutureResult<()> {
                        let db = ftry!(turtl2.create_user_db());
                        let mut db_guard = turtl2.db.write().unwrap();
                        *db_guard = Some(db);
                        drop(db_guard);
                        FOk!(())
                    })
                    .boxed()
            })
            .boxed()
    }
    */

    /// Log a user out
    pub fn logout(&self) -> TFutureResult<()> {
        self.with_next_fut()
            .and_then(|turtl| -> TFutureResult<()> {
                turtl.events.trigger("sync:shutdown", &Value::Bool(false));
                ftry!(User::logout(turtl.clone()));

                // wipe the user db
                let mut db_guard = turtl.db.write().unwrap();
                *db_guard = None;
                FOk!(())
            })
            .boxed()
    }

    /// Given that our API is synchronous but we need to not block the main
    /// thread, we wrap it here such that we can do all the setup/teardown of
    /// handing the Api object off to a closure that runs inside of our `async`
    /// runner.
    pub fn with_api<F>(&self, cb: F) -> TFutureResult<Value>
        where F: FnOnce(Arc<Api>) -> TResult<Value> + Send + Sync + 'static
    {
        let api = self.api.clone();
        self.async.run(move || {
            cb(api)
        })
    }

    /// Start our sync system. This should happen after a user is logged in, and
    /// we definitely have a Turtl.db object available.
    pub fn start_sync(&self) -> TResult<()> {
        // create the ol' in/out (in/out) db connections for our sync system
        let db_out = Arc::new(self.create_user_db()?);
        let db_in = Arc::new(self.create_user_db()?);
        // start the sync, and save the resulting state into Turtl
        let sync_state = sync::start(self.tx_main.clone(), self.sync_config.clone(), self.api.clone(), db_out, db_in)?;
        {
            let mut state_guard = self.sync_state.write().unwrap();
            *state_guard = Some(sync_state);
        }

        // set up some bindings to manage the sync system easier
        self.with_next(|turtl| {
            let turtl1 = turtl.clone();
            turtl.events.bind_once("app:shutdown", move |_| {
                turtl1.with_next(|turtl| {
                    turtl.events.trigger("sync:shutdown", &jedi::obj());
                });
            }, "turtl:app:shutdown:sync");

            let sync_state1 = turtl.sync_state.clone();
            let sync_state2 = turtl.sync_state.clone();
            let sync_state3 = turtl.sync_state.clone();
            turtl.events.bind_once("sync:shutdown", move |joinval| {
                let join = match *joinval {
                    Value::Bool(x) => x,
                    _ => false,
                };
                let mut guard = sync_state1.write().unwrap();
                if guard.is_some() {
                    let state = guard.as_mut().unwrap();
                    (state.shutdown)();
                    if join {
                        loop {
                            let hn = state.join_handles.pop();
                            match hn {
                                Some(x) => match x.join() {
                                    Ok(_) => (),
                                    Err(e) => error!("turtl -- sync:shutdown: problem joining thread: {:?}", e),
                                },
                                None => break,
                            }
                        }
                    }
                }
                *guard = None;
            }, "turtl:sync:shutdown");
            turtl.events.bind("sync:pause", move |_| {
                let guard = sync_state2.read().unwrap();
                if guard.is_some() { (guard.as_ref().unwrap().pause)(); }
            }, "turtl:sync:pause");
            turtl.events.bind("sync:resume", move |_| {
                let guard = sync_state3.read().unwrap();
                if guard.is_some() { (guard.as_ref().unwrap().resume)(); }
            }, "turtl:sync:resume");
            let turtl2 = turtl.clone();
            turtl.events.bind("sync:incoming:init:done", move |err| {
                // don't load the profile if we didn't sync correctly
                match *err {
                    Value::Bool(_) => {},
                    _ => return error!("turtl::sync:incoming:init:done -- sync error, skipping profile load"),
                }
                let turtl3 = turtl2.clone();
                let turtl4 = turtl2.clone();
                let runme = turtl2.load_profile()
                    .and_then(move |_| {
                        ftry!(Messenger::event("profile:loaded", jedi::obj()));
                        turtl3.index_notes()
                    })
                    .and_then(|_| {
                        ftry!(Messenger::event("profile:indexed", jedi::obj()));
                        FOk!(())
                    })
                    .or_else(move |e| -> TFutureResult<()> {
                        error!("turtl -- sync:load-profile: problem loading profile: {}", e);
                        ftry!(turtl4.error_event(&e, "load_profile"));
                        FOk!(())
                    });
                util::future::run(runme);
            }, "sync:incoming:init:done");
        });
        Ok(())
    }

    /// Create a new per-user database for the current user.
    pub fn create_user_db(&self) -> TResult<Storage> {
        let db_location = self.get_user_db_location()?;
        let dumpy_schema = config::get::<Value>(&["schema"])?;
        Storage::new(&db_location, dumpy_schema)
    }

    /// Get the physical location of the per-user database file we will use for
    /// the current logged-in user.
    pub fn get_user_db_location(&self) -> TResult<String> {
        let user_guard = self.user.read().unwrap();
        let user_id = match user_guard.id() {
            Some(x) => x,
            None => return Err(TError::MissingData(String::from("turtl.get_user_db_location() -- user.id() is None (can't open db without an ID)"))),
        };
        let data_folder = config::get::<String>(&["data_folder"])?;
        if data_folder == ":memory:" {
            return Ok(String::from(":memory:"));
        }
        let api_endpoint = config::get::<String>(&["api", "endpoint"])?;
        let re = Regex::new(r"(?i)[^a-z0-9]")?;
        let server = re.replace_all(&api_endpoint, "");
        Ok(format!("{}/turtl-user-{}-srv-{}.sqlite", data_folder, user_id, server))
    }

    /// Given a model that we suspect we have a key entry for, find that model's
    /// key, set it into the model, and return a reference to the key.
    pub fn find_model_key<T>(&self, model: &mut T) -> TResult<()>
        where T: Protected + Keyfinder
    {
        // check if we have a key already. if you're trying to re-find the key,
        // make sure you model.set_key(None) before calling...
        if model.key().is_some() { return Ok(()); }

        let notfound = Err(TError::NotFound(format!("key for {:?} not found", model.id())));

        /// A standard "found a key" function
        fn found_key<T>(model: &mut T, key: Key) -> TResult<()>
            where T: Protected
        {
            model.set_key(Some(key));
            return Ok(());
        }

        // fyi ders, this read lock is going to be open until we return
        let profile_guard = self.profile.read().unwrap();
        let ref keychain = profile_guard.keychain;

        // check the keychain right off the bat. it's quick and easy, and most
        // entries are going to be here anyway
        if model.id().is_some() {
            match keychain.find_entry(model.id().unwrap()) {
                Some(key) => return found_key(model, key),
                None => {},
            }
        }

        // ok, next up is to generate our search. essentially, each model passes
        // back a separate keychain that we can use to find keys within. for
        // instance, a Note might hand back a keychain with entries for each
        // Board it's in, allowing us to decrypt the Note's key from its
        // Note.keys collection using one of the board keys. note (ha ha) that
        // unlike the old Turtl, if we find a key and it *fails*, we just keep
        // looping until we find a working match or we exhaust our search. this
        // is a much more versatile way of decrypting data.
        let mut search = model.get_key_search(self);

        // grab the model.keys collection.
        let encrypted_keys: Vec<HashMap<String, String>> = match model.get_keys() {
            Some(x) => x.clone(),
            None => Vec::new(),
        };

        // if we have no self-decrypting keys, and there's no keychain entry for
        // this model, then there's no way we can find a key
        if encrypted_keys.len() == 0 { return notfound; }

        // turn model.keys into a KeyRef collection, and filter out crappy
        // entries
        let encrypted_keys: Vec<KeyRef<String>> = encrypted_keys.into_iter()
            .map(|entry| keychain::keyref_from_encrypted(&entry))
            .filter(|x| x.k != "")
            .collect::<Vec<_>>();

        // push the user's key into our search, if it's available
        {
            let user_guard = self.user.read().unwrap();
            if user_guard.id().is_some() && user_guard.key().is_some() {
                search.add_key(user_guard.id().unwrap(), user_guard.id().unwrap(), user_guard.key().unwrap(), &String::from("user"));
            }
        }

        // let the hunt begin! basically, we loop over each model.keys entry and
        // try to find that key item's key and use it to decrypt the model's
        // key. i know this sounds confusing, so take the following:
        //
        //   model:
        //     body: <encrypted data>
        //     keys:
        //       - {board: 1234, key: "b50942fe"}
        //   board:
        //     id: 1234
        //     key: "696969"
        // 
        // so our `model` has a key, "b50942fe". this key is encrypted, and the
        // only way to decrypt it is to use the board's key. so in the following
        // search, we look for a key for board 1234 both in our user's global
        // keychain *and* in the model's search keychain. if we find a match, we
        // can use the board's key, "696969", to decrypt the model's key entry
        // "b50942fe" into the model's actual key.
        for keyref in &encrypted_keys {
            let ref encrypted_key = keyref.k;
            let ref object_id = keyref.id;

            // check if this object is in the keychain first. if so, we can use
            // its key to decrypt our encrypted key
            match keychain.find_entry(object_id) {
                Some(decrypting_key) => {
                    match protected::decrypt_key(&decrypting_key, encrypted_key) {
                        Ok(key) => return found_key(model, key),
                        Err(_) => {},
                    }
                },
                None => {},
            }

            // check our search object for matches
            let matches = search.find_all_entries(object_id);
            for key in &matches {
                match protected::decrypt_key(key, encrypted_key) {
                    // it worked!
                    Ok(key) => return found_key(model, key),
                    // got an error...oh well. MUSH
                    Err(_) => {},
                }
            }
        }
        notfound
    }

    /// Given a model vector that we suspect we have a key entry for, find those
    /// models' keys and set it into the models.
    pub fn find_models_keys<T>(&self, models: &mut Vec<T>) -> TResult<()>
        where T: Protected + Keyfinder
    {
        let mut errcount = 0;
        for model in models {
            match self.find_model_key(model) {
                Ok(_) => {},
                Err(_) => {
                    error!("turtl.find_models_keys() -- skipping model {:?}: problem finding key", model.id());
                    errcount += 1;
                },
            }
        }
        if errcount > 0 {
            error!("turtl.find_models_keys() -- load summary: couldn't load keys for {} models", errcount);
        }
        Ok(())
    }

    /// Load the profile from disk.
    ///
    /// Meaning, we decrypt the keychain, spaces, and boards and store them
    /// in-memory in our `turtl.profile` object.
    pub fn load_profile(&self) -> TFutureResult<()> {
        let user_key = {
            let user_guard = self.user.read().unwrap();
            match user_guard.key() {
                Some(x) => x.clone(),
                None => return FErr!(TError::MissingData(String::from("turtl.load_profile() -- missing user key"))),
            }
        };
        let db_guard = self.db.write().unwrap();
        if db_guard.is_none() {
            return FErr!(TError::MissingData(String::from("turtl.load_profile() -- turtl.db is not initialized")));
        }
        let db = db_guard.as_ref().unwrap();
        let mut keychain: Vec<KeychainEntry> = ftry!(jedi::from_val(ftry!(jedi::to_val(&ftry!(db.all("keychain"))))));
        let mut spaces: Vec<Space> = ftry!(jedi::from_val(ftry!(jedi::to_val(&ftry!(db.all("spaces"))))));
        let mut boards: Vec<Board> = ftry!(jedi::from_val(ftry!(jedi::to_val(&ftry!(db.all("boards"))))));

        // keychain entries are always encrypted with the user's key
        for key in &mut keychain { key.set_key(Some(user_key.clone())); }

        // grab a clonable turtl context
        self.with_next_fut()
            .and_then(move |turtl| {
                let turtl1 = turtl.clone();
                let turtl2 = turtl.clone();
                let turtl3 = turtl.clone();
                // decrypt the keychain
                protected::map_deserialize(turtl.as_ref(), keychain)
                    .and_then(move |keychain: Vec<KeychainEntry>| -> TFutureResult<Vec<Space>> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl1.profile.write().unwrap();
                        profile_guard.keychain.entries = keychain;
                        drop(profile_guard);
                        // now decrypt the spaces
                        ftry!(turtl1.find_models_keys(&mut spaces));
                        protected::map_deserialize(turtl1.as_ref(), spaces)
                    })
                    .and_then(move |spaces: Vec<Space>| -> TFutureResult<Vec<Board>> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl2.profile.write().unwrap();
                        profile_guard.spaces = spaces;
                        drop(profile_guard);
                        // now decrypt the boards
                        ftry!(turtl2.find_models_keys(&mut boards));
                        protected::map_deserialize(turtl2.as_ref(), boards)
                    })
                    .and_then(move |boards: Vec<Board>| -> TFutureResult<()> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl3.profile.write().unwrap();
                        profile_guard.boards = boards;
                        drop(profile_guard);

                        turtl3.events.trigger("profile:loaded", &jedi::obj());
                        FOk!(())
                    })
                    .boxed()
            })
            .boxed()
    }

    /// Load/deserialize a set of notes by id.
    pub fn load_notes(&self, note_ids: &Vec<String>) -> TFutureResult<Vec<Note>> {
        let db_guard = self.db.read().unwrap();
        if db_guard.is_none() {
            return FErr!(TError::MissingField(String::from("turtl.load_notes() -- turtl is missing `db` object")));
        }
        let db = db_guard.as_ref().unwrap();

        let mut notes: Vec<Note> = ftry!(jedi::from_val(Value::Array(ftry!(db.by_id("notes", note_ids)))));
        ftry!(self.find_models_keys(&mut notes));
        self.with_next_fut()
            .and_then(move |turtl| -> TFutureResult<Vec<Note>> {
                protected::map_deserialize(turtl.clone().as_ref(), notes)
            })
            .boxed()
    }

    /// Take all the (encrypted) notes in our profile data then decrypt, index,
    /// and free them. The idea is we can get a set of note IDs from a search,
    /// but we're not holding all our notes decrypted in memory at all times.
    pub fn index_notes(&self) -> TFutureResult<()> {
        let db_guard = self.db.write().unwrap();
        if db_guard.is_none() {
            return FErr!(TError::MissingData(String::from("turtl.index_notes() -- turtl.db is not initialized")));
        }
        let db = db_guard.as_ref().unwrap();
        let mut notes: Vec<Note> = ftry!(jedi::from_val(ftry!(jedi::to_val(&ftry!(db.all("notes"))))));
        ftry!(self.find_models_keys(&mut notes));
        self.with_next_fut()
            .and_then(move |turtl| {
                let turtl1 = turtl.clone();
                protected::map_deserialize(turtl.as_ref(), notes)
                    .and_then(move |notes: Vec<Note>| -> TFutureResult<()> {
                        let search = ftry!(Search::new());
                        for note in &notes {
                            match search.index_note(note) {
                                Ok(_) => {},
                                // keep going on error
                                Err(e) => error!("turtl.index_notes() -- problem indexing note {:?}: {}", note.id(), e),
                            }
                        }
                        let mut search_guard = turtl1.search.write().unwrap();
                        *search_guard = Some(search);
                        FOk!(())
                    })
                    .or_else(|e| -> TFutureResult<()> {
                        error!("turtl.index_notes() -- there was a problem indexing notes: {}", e);
                        FErr!(e)
                    })
                    .boxed()
            })
            .boxed()
    }

    /// Run the given callback on the next main loop. Essentially gives us a
    /// setTimeout (if you are familiar). This means we can do something after
    /// the stack is unwound, but get a fresh Turtl context for our callback.
    ///
    /// Very useful for (un)binding events and such while inside of another
    /// triggered event (which normally deadlocks).
    ///
    /// Also note that this doesn't call the `cb` with `Turtl`, but instead
    /// `TurtlWrap` which is also nice because we can `.clone()` it and use it
    /// in multiple callbacks.
    pub fn with_next<F>(&self, cb: F)
        where F: FnOnce(TurtlWrap) + Send + Sync + 'static
    {
        self.tx_main.next(cb);
    }

    /// Return a future that resolves with a TurtlWrap object on the next main
    /// loop.
    pub fn with_next_fut(&self) -> TFutureResult<TurtlWrap> {
        self.tx_main.next_fut()
    }

    /// Log out the current user (if logged in) and wipe ALL local SQL databases
    /// from our data folder.
    pub fn wipe_local_data(&self) -> TResult<()> {
        let mut kv_guard = self.kv.write().unwrap();
        kv_guard.close()?;
        let data_folder = config::get::<String>(&["data_folder"])?;
        let paths = fs::read_dir(data_folder)?;
        for entry in paths {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() { continue; }
            let filename = entry.file_name();
            let filename_str = match filename.to_str() {
                Some(x) => x,
                None => return Err(TError::Msg(format!("turtl.wipe_local_data() -- error converting OsString into &str"))),
            };
            if &filename_str[0..6] != "turtl-" { continue; }
            fs::remove_file(&path)?;
            info!("turtl.wipe_local_data() -- removing {}", path.display());
        }
        (*kv_guard) = Turtl::open_kv()?;
        Ok(())
    }

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) { }
}

// Probably don't need this since `shutdown` just wipes our internal state which
// would happen anyway it Turtl is dropped, but whatever.
impl Drop for Turtl {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ::std::sync::{Arc, RwLock};

    use ::futures::Future;

    use ::config;
    use ::jedi;

    use ::error::TFutureResult;
    use ::crypto::{self, Key};
    use ::search::Query;
    use ::util;
    use ::util::thredder::Pipeline;
    use ::models::model::Model;
    use ::models::protected::Protected;
    use ::models::keychain::KeychainEntry;
    use ::models::user::{self, User};
    use ::models::note::Note;
    use ::models::board::Board;
    use ::util::stopper::Stopper;

    protected!{
        pub struct Dog {
            ( user_id: String ),
            ( name: String ),
            ( )
        }
    }

    /// Give us a new Turtl to start running tests on
    fn with_test(logged_in: bool) -> Turtl {
        config::set(&["data_folder"], &String::from(":memory:")).unwrap();
        let turtl = Turtl::new(Pipeline::new()).unwrap();
        if logged_in {
            let mut user_guard = turtl.user.write().unwrap();
            let version = 0;    // version 0 is much quicker...
            let (key, auth) = user::generate_auth(&String::from("timmy@killtheradio.net"), &String::from("gfffft"), version).unwrap();
            user_guard.id = Some(String::from("0158745252dbaf227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba033e1"));
            user_guard.do_login(key, auth);
        }
        turtl
    }

    #[test]
    fn finding_keys() {
        let note_key = Key::new(crypto::from_base64(&String::from("eVWebXDGbqzDCaYeiRVsZEHsdT5WXVDnL/DdmlbqN2c=")).unwrap());
        let board_key = Key::new(crypto::from_base64(&String::from("BkRzt6lu4YoTS9opB96c072y+kt+evtXv90+ZXHfsG8=")).unwrap());
        let enc_board = String::from(r#"{"body":"AAUCAAHeI0ysDNAenXpPAlOwQmmHzNWcohaCSmRXOPiRGVojaylzimiohTBSG2DyPnfsSXBl+LfxXhA=","keys":[],"user_id":"5244679b2b1375384f0000bc","id":"01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c"}"#);
        let enc_note = String::from(r#"{"board_id":"01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c","mod":1479425965,"keys":[{"b":"01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c","k":"AAUCAAECDLI141jXNUwVadmvUuxXYtWZ+JL7450VjH1JURk0UigiIB2TQ2f5KiDGqZKUoHyxFXCaAeorkaXKxCaAqicISg=="}],"user_id":"5244679b2b1375384f0000bc","body":"AAUCAAGTaDVBJHRXgdsfHjrI4706aoh6HKbvoa6Oda4KP0HV07o4JEDED/QHqCVMTCODJq5o2I3DNv0jIhZ6U3686ViT6YIwi3EUFjnE+VMfPNdnNEMh7uZp84rUaKe03GBntBRNyiGikxn0mxG86CGnwBA8KPL1Gzwkxd+PJZhPiRz0enWbOBKik7kAztahJq7EFgCLdk7vKkhiTdOg4ghc/jD6s9ATeN8NKA90MNltzTIM","id":"015874a823e4af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0361f"}"#);
        let mut board: Board = jedi::parse(&enc_board).unwrap();
        let mut note: Note = jedi::parse(&enc_note).unwrap();

        let turtl = with_test(true);
        let user_id = {
            let user_guard = turtl.user.read().unwrap();
            user_guard.id().unwrap().clone()
        };

        // add the note's key as a direct entry to the keychain
        let mut profile_guard = turtl.profile.write().unwrap();
        profile_guard.keychain.add_key(&user_id, note.id().unwrap(), &note_key, &String::from("note"));
        drop(profile_guard);

        // see if we can find the note as a direct entry
        {
            turtl.find_model_key(&mut note).unwrap();
            let found_key = note.key().unwrap();
            assert_eq!(found_key, &note_key);
        }

        // clear out the keychain, and add the board's key to the keychain
        let mut profile_guard = turtl.profile.write().unwrap();
        profile_guard.keychain.entries.clear();
        assert_eq!(profile_guard.keychain.entries.len(), 0);
        profile_guard.keychain.add_key(&user_id, board.id().unwrap(), &board_key, &String::from("board"));
        assert_eq!(profile_guard.keychain.entries.len(), 1);
        drop(profile_guard);

        // we should be able to find the board's key, if we found the note's key
        // but it's good to be sure
        {
            turtl.find_model_key(&mut board).unwrap();
            let found_key = board.key().unwrap();
            assert_eq!(found_key, &board_key);
        }

        // ok, now the real test...can we find the note's key by one layer of
        // indirection? (in other words, the note has no keychain entry, so it
        // searches the keychain for it's note.keys.b record, and uses that key
        // (if found) to decrypt its own key
        {
            turtl.find_model_key(&mut note).unwrap();
            let found_key = note.key().unwrap();
            assert_eq!(found_key, &note_key);
        }

        // clear out the keychain. we're going to see if the note's
        // get_key_search() function works for us
        let mut profile_guard = turtl.profile.write().unwrap();
        profile_guard.keychain.entries.clear();
        // put the board into the profile
        profile_guard.boards.push(board);
        assert_eq!(profile_guard.keychain.entries.len(), 0);
        drop(profile_guard);

        // empty keychain...this basically forces the find_model_key() fn to
        // use the model's get_key_search() function, which is custom for the
        // note type to search based on board keys
        {
            turtl.find_model_key(&mut note).unwrap();
            let found_key = note.key().unwrap();
            assert_eq!(found_key, &note_key);
        }
    }

    #[test]
    fn loads_profile_search_notes() {
        let stop = Arc::new(Stopper::new());
        stop.set(true);

        let user_key = Key::new(crypto::from_base64(&String::from("sYIjvIDIEqcEbDOw7N+e61zqT1Z2Tgeu7++k8bqxJEc=")).unwrap());
        let mut user: User = jedi::parse(&String::from(r#"{"id":"5897a27c2b137539ea00a77d","storage":104857600}"#)).unwrap();
        let user_auth = String::from("AAUCAAFlNTk1NzM0OGY2ZTk2MWQ3l9HyiA+B8vtiI0V6X70LZDx60RfPbvciVPFyHROrXg7G32GKzT2/i83BXNMiLTKZInF/tO2BkdNn4yROW/rkP74dnkVk6v4WYzDTNkm1wWjgVnHGWVpCoUXVlx6NJjbuQgbZmL7L+RSvYOvXfkmDdsEpznPHKDTiHu0Q63erajyqRaIhCBweW81Ug3/yGquBxj4=");
        user.do_login(user_key, user_auth);

        let mut turtl = with_test(false);
        turtl.user = RwLock::new(user);

        let db = turtl.create_user_db().unwrap();
        // load our profile from a few big JSON blobs. we do this out of scope
        // so's not to be tempted to use them later on...we want the profile to
        // load itself completely from the DB and deserialize successfully w/o
        // having access to any of the data we put in here.
        {
            let keychain: Vec<KeychainEntry> = jedi::parse(&String::from(r#"[{"type":"board","item_id":"015a10534f43b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901b2","body":"AAUCAAGPTF7oW45ES+n+0lVJcnrOxW5gbwYk5mLJ1lSs1FAD6VpNEV7d0q9yQdAB5hnlTk9yM8TYaVVmq2AjdXVeZkbYQA9trMQZdXwrqz+EZLa0sjhN/W9i","id":"015a10536984b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901ba","user_id":"5897a27c2b137539ea00a77d"},{"type":"board","item_id":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5","body":"AAUCAAFULkds49e7jMhfSmdnpr1iAEb4mSOLAq/7bktqiUw+fo+Vr4qrbOiVwBZI9MShfC1XWr1AWYfyfB5eSQppgdzCRgYWHxvzzbfYMTpwoJcfaGfH6xiA","id":"015a105381ccb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901d3","user_id":"5897a27c2b137539ea00a77d"},{"type":"space","item_id":"015a1054c65bb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e1","body":"AAUCAAEedoik2RcvQGLM4yYvHPDm7ErrjlQGrp3y6bf//XDsHKzwcrI+4CT3Wg9W75SXddF+4kk3h6GCjLo4coBHqXnsY978WWTqzImgMaXk3nS9+3o2Znhy","id":"015a1056404db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e3","user_id":"5897a27c2b137539ea00a77d"},{"type":"note","item_id":"015a105725ffb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901ef","body":"AAUCAAE69FJ9mWRvsiQOsIjTB6B1HoLWhBk2ETOjplNLf61zKr4FOkwVvMud5mqX83WusbtovNGGcqW2fknsiVkLZAwxApkiWR8s/rITZ4YhTLvihHhmwR7Z","id":"015a1059fee6b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901fb","user_id":"5897a27c2b137539ea00a77d"},{"type":"note","item_id":"015a105a0e63b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90211","body":"AAUCAAG0B4wGKxVcirY9kux58vxymBDJoC99VujFNcaTnu8BqZtaKFM+ACYqWHNUPpbqkH/TIt1wCkxGE05bUEhp7UVrm4nFRN8JbaKBG4pHLYy1+t2CTMQG","id":"015a105afe7eb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9021d","user_id":"5897a27c2b137539ea00a77d"},{"type":"note","item_id":"015a105b1dafb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90246","body":"AAUCAAEwdKz7v4jRwE2Qq3usi2aaCV9BmESD7vLOkTJDzU2hSVwosyFWxrLEfAYk8R8x9XR9dFmfGtNC0cL1zMZzklZY57yBgmCX9Ju4zmoRS5C5bW1Vz83k","id":"015a105b442ab6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90252","user_id":"5897a27c2b137539ea00a77d"}]"#)).unwrap();
            let spaces: Vec<Space> = jedi::parse(&String::from(r#"[{"id":"015a1054c65bb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e1","body":"AAUCAAGCipmWS5ywu4O+OirUhZ/buwimSzQKMOJVfjE3DJrfcVJw2EOrMlipUlFYmjmshbYX117WlQ==","user_id":"5897a27c2b137539ea00a77d"}]"#)).unwrap();
            let boards: Vec<Board> = jedi::parse(&String::from(r#"[{"keys":[],"user_id":"5897a27c2b137539ea00a77d","body":"AAUCAAHJmBzjfB1qIpWY/T3wVPcYxrj6KABZrUAG5gHHPOdUaouz2hKPbkdU2C1MATuM2Q7T/A==","id":"015a10534f43b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901b2"},{"keys":[],"user_id":"5897a27c2b137539ea00a77d","body":"AAUCAAHz2Gvszvv9lPaww/GoJ8/t7fAmXGrxIPoUdTRHVTwl07cq23mg6F0xolee/4v3D0K5FSOaeZaKawrNc+YcV3s=","id":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5"}]"#)).unwrap();
            let notes: Vec<Note> = jedi::parse(&String::from(r#"[{"mod":1486333616,"keys":[{"b":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5","k":"AAUCAAE6F6rFjwn3LzlqDtaGk3qOXewSrV71LcS5G2go5EMGcaY/k7V4tYkcYB8kmUmAcFezUBuhfG3EWCsRSgf3V7+NSQ=="}],"user_id":"5897a27c2b137539ea00a77d","body":"AAUCAAF+t+NvCEfMCA580A0M4e0HxHBo+UBPiU0tLluju4gDYRH0c5CqxHcOqC/+rSQpIpql8FkBji9UVQvvUA3FqcpRg0VEis4ptAsE/4cc0H+1nxnTb8GkPNjs/M7zXgo3vxm9kSY3ChXegomYBTkoom8O/namIiNiydzC0pSVXkWlMPtp9FMN8BEhD5TJvE9phBnxORpz/GqE3yg/wfTwNuND5fpb0Lic0SDLHfe1fVMq6eqNbhXXCkc5+ynkS9LjcVtaggOGYg==","id":"015a105725ffb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901ef","board_id":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5","space_id":"015a1054c65bb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e1"},{"mod":1486333781,"keys":[{"b":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5","k":"AAUCAAEWj1hWQKMmeb1vtn2yr15dLLjpJZL6fzqaz3Y9yCXQhWTWhMXPu0dP49Xl1dinaRvCr4KVadbRgw2c6B3h+KvklQ=="}],"user_id":"5897a27c2b137539ea00a77d","body":"AAUCAAG3T7Olqr0BffKxTZhv1nG60NX8e0GHb7qoV+FRKSTWs7GraIn5oeJVa2DMKyIUPKKfnh9ozJ8K4STC8whcgH59J15YPStodC+Qf8QG1MhWIScUu9PxLKJ6Yw5yyf71/TIXjKP1hj38kk8qkOufGJurSAOhQoUM0YkdZ2/1hAIvR5sEdaYldm9cWmVWbp0VfKlo6RvZgs1ZQc8V57Oi2geycnUTsdqve2OP3i6N6L1lvbcnkdjYPDz/VHjcRCE1sODkUcZtHz1uwnrMASNssbZ7XHWya3lFGS2t0k2GIofhfixfgLUA/cveNDvsWziaojWKMiDP03C3iEWuSXEid8+LgB1txvOXzO8zoPHTWs/+mdiylP138G8ttwhCAnqEkd0=","id":"015a105a0e63b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90211","board_id":"015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5","space_id":"015a1054c65bb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e1"},{"mod":1486333102,"keys":[{"b":"015a10534f43b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901b2","k":"AAUCAAGX6aQGVo+B8guA8rHUPc5+Z+NSW1QDZM/BI9TLx7W3jDnx0Lp7aX9YbT915bi7DFUSrZ4C5gu1Pa4IFpJRNEfF8w=="}],"user_id":"5897a27c2b137539ea00a77d","body":"AAUCAAHqU7lGQFSd2i5PDr3pSRmOxcgAOW3IFkQcn4gU8GINA1s97M0Whyy4/4MkVSWj8dFCfPdZySCVWrbNcZhpkjBTvmvgG/aI55keLRx5aY6jJ8iC0K2NP3bQ+Go0ZCWLGU99oWQfnTeA8GBTHrZ54y+jxX3Bk/VZ9pJmv4KslS1nVRQ65YvZ3umjyJNcBwMjIhQ=","id":"015a105b1dafb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90246","board_id":"015a10534f43b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901b2","space_id":"015a1054c65bb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901e1"}]"#)).unwrap();

            for entry in &keychain { db.save(entry).unwrap(); }
            for space in &spaces { db.save(space).unwrap(); }
            for board in &boards { db.save(board).unwrap(); }
            for note in &notes { db.save(note).unwrap(); }
        }
        turtl.db = RwLock::new(Some(db));

        let turtl = Arc::new(turtl);
        let ref tx_main = turtl.tx_main;
        let turtl2 = turtl.clone();
        let turtl3 = turtl.clone();
        let tx2 = turtl.tx_main.clone();
        let stop2 = stop.clone();
        let runme = turtl.load_profile()
            .and_then(move |_| {
                let profile_guard = turtl2.profile.read().unwrap();
                assert_eq!(profile_guard.keychain.entries.len(), 6);
                assert_eq!(profile_guard.spaces.len(), 1);
                assert_eq!(profile_guard.boards.len(), 2);
                assert_eq!(profile_guard.boards[1].title.as_ref().unwrap(), &String::from("things i dont like"));
                turtl2.index_notes()
            })
            .and_then(move |_| {
                fn parserrr(json: &str) -> Query {
                    jedi::parse(&String::from(json)).unwrap()
                }

                let search_guard = turtl3.search.read().unwrap();
                let search = search_guard.as_ref().unwrap();

                // this stuff is mostly covered in the search tests, but let's
                // just make sure here.

                let qry = parserrr(r#"{"boards":["015a10537078b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901c5"]}"#);
                assert_eq!(search.find(&qry).unwrap(), vec![String::from("015a105a0e63b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90211"), String::from("015a105725ffb6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c901ef")]);

                let qry = parserrr(r#"{"text":"story deployment"}"#);
                assert_eq!(search.find(&qry).unwrap(), vec![String::from("015a105a0e63b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90211")]);

                let qry = parserrr(r#"{"text":"story baby"}"#);
                assert_eq!(search.find(&qry).unwrap().len(), 0);

                assert_eq!(
                    search.tags_by_frequency(&Vec::new(), 9999).unwrap(),
                    vec![
                        (String::from("morons"), 1),
                        (String::from("programmers"), 1),
                        (String::from("story"), 1),
                    ]
                );
                FOk!(())
            })
            .or_else(|e| -> TFutureResult<()> {
                panic!("load profile error: {}", e);
            })
            .then(move |_| -> TFutureResult<()> {
                stop2.set(false);
                tx2.next(|_| {});
                FOk!(())
            });
        util::future::run(runme);
        util::future::start_poll(tx_main.clone());
        while stop.running() {
            let handler = tx_main.pop();
            handler.call_box(turtl.clone());
        }
    }
}

