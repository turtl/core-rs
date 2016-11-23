//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info, and is passed
//! around to various pieces of the app running in the main thread.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;
use ::std::collections::HashMap;
use ::std::mem;

use ::regex::Regex;
use ::futures::Future;
use ::num_cpus;

use ::jedi::{self, Value};
use ::config;

use ::error::{TResult, TFutureResult, TError};
use ::crypto::Key;
use ::util::event::{self, Emitter};
use ::storage::{self, Storage};
use ::api::Api;
use ::profile::Profile;
use ::models::protected::{self, Keyfinder, Protected};
use ::models::model::Model;
use ::models::user::User;
use ::models::board::Board;
use ::models::persona::Persona;
use ::models::keychain::{self, KeyRef, KeychainEntry};
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::{Messenger, Response};
use ::sync::{self, SyncConfig, SyncState};

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
    pub kv: Arc<Storage>,
    /// Our main database, initialized after a successful login. This db is
    /// named via a function of the user ID and the server we're talking to,
    /// meaning we can have multiple databases that store different things for
    /// different people depending on server/user.
    pub db: RwLock<Option<Storage>>,
    /// Our external API object. Note that most things API-related go through
    /// the Sync system, but there are a handful of operations that Sync doesn't
    /// handle that need API access (Personas (soon to be deprecated) and
    /// invites come to mind). Use sparingly.
    pub api: Arc<Api>,
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
        let data_folder = config::get::<String>(&["data_folder"])?;
        let kv_location = if data_folder == ":memory:" {
            String::from(":memory:")
        } else {
            format!("{}/kv.sqlite", &data_folder)
        };
        let kv = Arc::new(Storage::new(&kv_location, jedi::obj())?);

        // make sure we have a client id
        storage::setup_client_id(kv.clone())?;

        let turtl = Turtl {
            tx_main: tx_main.clone(),
            events: event::EventEmitter::new(),
            user: RwLock::new(User::new()),
            profile: RwLock::new(Profile::new()),
            api: api,
            msg: Messenger::new(),
            work: Thredder::new("work", tx_main.clone(), num_workers as u32),
            async: Thredder::new("async", tx_main.clone(), 2),
            kv: kv,
            db: RwLock::new(None),
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
                        let db = try_fut!(turtl2.create_user_db());
                        let mut db_guard = turtl2.db.write().unwrap();
                        *db_guard = Some(db);
                        drop(db_guard);
                        FOk!(())
                    })
                    .boxed()
            })
            .boxed()
    }

    /// Log a user out
    pub fn logout(&self) -> TFutureResult<()> {
        self.with_next_fut()
            .and_then(|turtl| -> TFutureResult<()> {
                turtl.events.trigger("sync:shutdown", &Value::Bool(false));
                try_fut!(User::logout(turtl.clone()));

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
            turtl.events.bind("sync:incoming:init:done", move |_| {
                let turtl3 = turtl2.clone();
                turtl2.load_profile()
                    .or_else(move |e| -> TFutureResult<()> {
                        error!("turtl -- sync:load-profile: problem loading profile: {}", e);
                        try_fut!(turtl3.error_event(&e, "load_profile"));
                        FOk!(())
                    })
                    .forget();
            }, "sync:incoming:init:done");
            turtl.events.bind("profile:loaded", move |_| {
                match Messenger::event("profile:loaded", jedi::obj()) {
                    Ok(_) => {},
                    Err(e) => error!("turtl -- profile:loaded: problem sending event: {}", e),
                }
            }, "turtl:profile:loaded");
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
    pub fn find_model_key<'a, T>(&self, model: &'a mut T) -> TResult<()>
        where T: Protected + Keyfinder
    {
        // check if we have a key already. if you're trying to re-find the key,
        // make sure you model.set_key(None) before calling...
        if model.key().is_some() { return Ok(()); }

        let notfound = Err(TError::NotFound(format!("key for {:?} not found", model.id())));

        /// A standard "found a key" function
        fn found_key<'a, T>(model: &'a mut T, key: Key) -> TResult<()>
            where T: Protected
        {
            model.set_key(Some(key));
            return Ok(());
        }

        // fyi, this read lock is going to be open until we return
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

    /// Load the profile from disk.
    ///
    /// Meaning, we decrypt the keychain, boards, and personas and store them
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
        let mut keychain: Vec<KeychainEntry> = try_fut!(jedi::from_val(jedi::to_val(&try_fut!(db.all("keychain")))));
        let mut boards: Vec<Board> = try_fut!(jedi::from_val(jedi::to_val(&try_fut!(db.all("boards")))));
        let mut personas: Vec<Persona> = try_fut!(jedi::from_val(jedi::to_val(&try_fut!(db.all("personas")))));

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
                    .and_then(move |keychain: Vec<KeychainEntry>| -> TFutureResult<Vec<Board>> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl1.profile.write().unwrap();
                        profile_guard.keychain.entries = keychain;
                        drop(profile_guard);

                        // now decrypt the boards
                        for board in &mut boards { try_fut!(turtl1.find_model_key(board)); }
                        protected::map_deserialize(turtl1.as_ref(), boards)
                    })
                    .and_then(move |boards: Vec<Board>| -> TFutureResult<Vec<Persona>> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl2.profile.write().unwrap();
                        profile_guard.boards = boards;
                        drop(profile_guard);

                        // now decrypt the personas
                        for persona in &mut personas { persona.set_key(Some(user_key.clone())); }
                        protected::map_deserialize(turtl2.as_ref(), personas)
                    })
                    .and_then(move |mut personas: Vec<Persona>| -> TFutureResult<()> {
                        // set the keychain into the profile
                        let mut profile_guard = turtl3.profile.write().unwrap();
                        if personas.len() > 0 {
                            let mut into = Persona::new();
                            mem::swap(&mut into, &mut personas[0]);
                            profile_guard.persona = Some(into);
                        } else {
                            profile_guard.persona = None;
                        }
                        drop(profile_guard);
                        turtl3.events.trigger("profile:loaded", &jedi::obj());
                        FOk!(())
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
    use ::util::thredder::Pipeline;
    use ::models::model::Model;
    use ::models::protected::Protected;
    use ::models::keychain::KeychainEntry;
    use ::models::user::{self, User};
    use ::models::note::Note;
    use ::models::board::Board;
    use ::models::persona::Persona;
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
        let enc_note = String::from(r#"{"boards":["01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c"],"mod":1479425965,"keys":[{"b":"01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c","k":"AAUCAAECDLI141jXNUwVadmvUuxXYtWZ+JL7450VjH1JURk0UigiIB2TQ2f5KiDGqZKUoHyxFXCaAeorkaXKxCaAqicISg=="}],"user_id":"5244679b2b1375384f0000bc","body":"AAUCAAGTaDVBJHRXgdsfHjrI4706aoh6HKbvoa6Oda4KP0HV07o4JEDED/QHqCVMTCODJq5o2I3DNv0jIhZ6U3686ViT6YIwi3EUFjnE+VMfPNdnNEMh7uZp84rUaKe03GBntBRNyiGikxn0mxG86CGnwBA8KPL1Gzwkxd+PJZhPiRz0enWbOBKik7kAztahJq7EFgCLdk7vKkhiTdOg4ghc/jD6s9ATeN8NKA90MNltzTIM","id":"015874a823e4af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0361f"}"#);
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
    fn loads_profile() {
        let stop = Arc::new(Stopper::new());
        stop.set(true);

        let user_key = Key::new(crypto::from_base64(&String::from("EdFc225pnjtUVaH+YMApzOWL0OgFTsjn5YvPWbvpN1Q=")).unwrap());
        let mut user: User = jedi::parse(&String::from(r#"{"keys":[],"settings":[{"key":"keys"}],"id":"5833e49e2b13751ab303f8b1","storage":104857600}"#)).unwrap();
        user.do_login(user_key, String::from("AAUCAAEyYzY1ZjFkNjY5MzQ2YmE20RFp2guhlblECuXJF9/W/ylBaN15MOM5tjse8SlvL70my76088X8Fkkg08WOqkfY3+jyICX34UI9rWQfvWMxbTFbXZpJfDuWD+j+PExpCRp2PXZqmB14VhttKxGipxSe3gCl5ZHVjH4gWWw9CbD+1I0Agm0nul6MkzEdCQIByIB5yduoozthduWlpsJgz5nYOUo="));

        let mut turtl = with_test(false);
        turtl.user = RwLock::new(user);

        let db = turtl.create_user_db().unwrap();
        {
            let keychain: Vec<KeychainEntry> = jedi::parse(&String::from(r#"[{"body":"AAUCAAFqj/FnzMVEFk20i0f4d/NY6TTMZ9jrCRWhheu2LJ/oRZrnWkLBMDmHV6EkOiPWNYjJYw+7mLhqR/mXbZrdx/CMv/DhrNCBwnc1BvepuusmwS6NIfeO","type":"board","item_id":"01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994","user_id":"5833e49e2b13751ab303f8b1","id":"01588ab72a77af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0699c"},{"body":"AAUCAAGOoWbH1M2YgBdt6eaF8nlk7Dt52mpPp2ZF7uixMkVgk3h50kqKdBoIbkcQz3NqUQLO9XECPnwDLaxgZKhLk3IW+hzxYVEuzQxE93J/F6m6patHmftN","type":"note","item_id":"01588ab73d32af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba069b1","user_id":"5833e49e2b13751ab303f8b1","id":"01588ab7e3e8af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba069bd"},{"body":"AAUCAAEr0ky+zDcT0G+3lmZdGNsn6YXZZFDGreDzzESc8wOO+/tHmCK3RxV6aAEhtvCKSwsCbq/oylRIYZoGv1uwAnu7zQOAkzBH6ioStNYN/bmmsXlSllUK","type":"note","item_id":"01588ab8f907af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a0c","user_id":"5833e49e2b13751ab303f8b1","id":"01588abc24a8af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a2d"},{"body":"AAUCAAETC4kRU+NGZUE+0lXt2wXP+ENkOPOzZ5X39j6bRkeNbz0VY4+VwIkGjYLJh21+urVOmUWan7S9QE9z3U+jJV5qZZqCDHnwJjMg7ztDQmfqa/aLoS1k","type":"note","item_id":"01588abc6345af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a56","user_id":"5833e49e2b13751ab303f8b1","id":"01588abd7fe5af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a61"}]"#)).unwrap();
            let personas: Vec<Persona> = jedi::parse(&String::from(r#"[{"user_id":"5833e49e2b13751ab303f8b1","email":"andrew+test@turtl.it","name":"Andrew Lyon","body":"AAUCAAGFeUICKVgIylzweY9G5cPDPrqueWkVcgcqrZXtpuWYtxZOoO8sa6pyKuzoSD487saGXSqxogx0Yza61rRMeM/gH9BUq5WCw+RnOxVOBuAzD0POEbJ2yWRHgGUGPBzD7WZvkWiMxfbncB5mEujv0103ZuGAoeA0d1cxKQfx8Ja1ZyZXGHv5","id":"01588ab51d51af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba068e5","pubkey":"so then, remember, *i* said... \"later dudes!!\""}]"#)).unwrap();
            let boards: Vec<Board> = jedi::parse(&String::from(r#"[{"body":"AAUCAAHEPNXZVpgP84GRN3xDVnlcMyo4DGiloJmJHLBkasXVFlQHsH3BUkmAA17rVkpZF9R/KBE7jwwBIW7O","keys":[],"user_id":"5833e49e2b13751ab303f8b1","id":"01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994"}]"#)).unwrap();
            let notes: Vec<Note> = jedi::parse(&String::from(r#"[{"boards":["01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994"],"mod":1479796124,"keys":[{"b":"01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994","k":"AAUCAAEYrjFG1IY44n0n09Ex6fUbsJMwHrkQiOgkXCx1/7sjcLn+2tk1zoPDgpujO05uFV9+m1g92AvFy4H0rzoNQhtxPw=="}],"user_id":"5833e49e2b13751ab303f8b1","body":"AAUCAAHyrflOwSatekp9uWRciF52AReRCbH8SnxWIQWWbvpg8okcD4ugdhPqdsLl7a0zHVyKvHwDprfAJixlYecrx8X6I3R/9HdZ+JyNTI2JLxKJWcc5YMFIfpNeEcHZgomnTAplBpR420e+NddpSSeLGp6/EZHPLnBMzwkITSR8i1YPJx2jya8gLvrOkqb9tfLh4snpbx+B7yJkGTzrXPsqQBC8fuNVtmzh4uV5b0swBE0uE5sRw9+TQBvcP7TIP2Oq4t8NEGptY5Raqt+MauZWybP+3+2165JFR+JW+eNn2vw9af3XmKY06D0g3gzBF2gyKTvtRRs7eQ7UC7ckl/It8vE0NbE=","id":"01588ab73d32af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba069b1"},{"boards":["01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994"],"mod":1479796345,"keys":[{"b":"01588ab62d05af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06994","k":"AAUCAAH4KSWqjSD6EYN0pUpjAufMhj/fQm1cfxaWlG9Q1+iedpR/efWGVTMFVwtUNLsM99Q1lmNz5+fmPqCAcdf2GRhyOw=="}],"user_id":"5833e49e2b13751ab303f8b1","body":"AAUCAAFyUX8bZlt7RaG8EZTIk0nVWUtqcOqI68DIX1gqhh6FEyArPwJRLCEgjQeERsVyegBf4kGQWTxcxE9FA59h/Dmb0yCtzYVlsKUB63I0FT3ZHbFSorNHisJI/ue+msiHarz5J2fQ9sYUAwHuPf7kVbIibVB6RkbZMHLcLCVHl8zoBP50YEoX+8j7S3MkIqqsfoU/txtprQXzkk2NNF5rm5C8KtAWSdaa/dz6ZObi/En/+2SknjJDMYh0JbCxM39wHoC48zP0gygy9PBuAql3Qp5FIVYXljWjwR9+7i17KsTreOa5gxMhRu3snMCefnhCbbnUnJWU0Vl4hbr+tvEfE+JJxxAvAq6t","id":"01588ab8f907af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a0c"},{"boards":[],"mod":1479796427,"keys":[],"user_id":"5833e49e2b13751ab303f8b1","body":"AAUCAAFUt4EhaYvKe4zpbr6rnEeswZPtm98RVLdHn+Dml+RX8cTZpfJHuWUA7vkybCgI7RnLL+hvYODO9H25jtfgLBfx/IfSOO1AGC+q4vqZQptpxCkbkeLsKuZFp0JPMBKuz9xX5/MX/lFtk4OvVZhlo0f/XdPoVD+F36zw6gizO2oLYkFbQWiA5oOAQtuJtuRO41pyDh76ciCULgqTjChqO3c8hpaoklk=","id":"01588abc6345af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba06a56"}]"#)).unwrap();

            for entry in &keychain { db.save(entry).unwrap(); }
            for persona in &personas { db.save(persona).unwrap(); }
            for board in &boards { db.save(board).unwrap(); }
            for note in &notes { db.save(note).unwrap(); }
        }
        turtl.db = RwLock::new(Some(db));

        let turtl = Arc::new(turtl);
        let ref tx_main = turtl.tx_main;
        let turtl2 = turtl.clone();
        let stop2 = stop.clone();
        turtl.load_profile()
            .and_then(move |_| {
                let profile_guard = turtl2.profile.read().unwrap();
                assert_eq!(profile_guard.keychain.entries.len(), 4);
                assert_eq!(profile_guard.boards.len(), 1);
                assert!(profile_guard.persona.is_some());
                FOk!(())
            })
            .or_else(|e| -> TFutureResult<()> {
                panic!("load profile error: {}", e);
            })
            .then(move |_| -> TFutureResult<()> {
                stop2.set(false);
                FOk!(())
            })
            .forget();
        while stop.running() {
            let handler = tx_main.pop();
            handler.call_box(turtl.clone());
        }
    }
}

