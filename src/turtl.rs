//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info, and is passed
//! around to various pieces of the app running in the main thread.

use ::std::sync::{Arc, RwLock, Mutex};
use ::std::ops::Drop;
use ::std::fs;
use ::regex::Regex;
use ::num_cpus;
use ::jedi::{self, Value};
use ::config;
use ::error::{TResult, TError};
use ::crypto::Key;
use ::util;
use ::util::thredder::Thredder;
use ::storage::{self, Storage};
use ::api::Api;
use ::profile::Profile;
use ::models::protected::{self, Keyfinder, Protected};
use ::models::model::Model;
use ::models::user::{self, User};
use ::models::space::Space;
use ::models::board::Board;
use ::models::invite::Invite;
use ::models::keychain::KeychainEntry;
use ::models::note::Note;
use ::models::file::FileData;
use ::models::sync_record::{SyncRecord, SyncAction};
use ::messaging::{self, Messenger, Response};
use ::sync::{self, SyncConfig, SyncState};
use ::sync::sync_model::MemorySaver;
use ::search::Search;
use ::schema;
use ::migrate::{self, MigrateResult};
use ::std::collections::HashMap;

pub fn data_folder() -> TResult<String> {
    let integration = config::get::<String>(&["integration_tests", "data_folder"])?;
    if cfg!(test) {
        return Ok(integration);
    }
    let data_folder = config::get::<String>(&["data_folder"])?;
    let final_folder = if data_folder == ":memory:" {
        integration
    } else {
        data_folder
    };
    Ok(final_folder)
}

/// Defines a container for our app's state. Note that most operations the user
/// has access to via messaging get this object passed to them.
pub struct Turtl {
    /// Holds our current user (Turtl only allows one logged-in user at once)
    pub user: RwLock<User>,
    /// A lot of times we just want to get the user's id. We shouldn't have to
    /// lock the `turtl.user` object just for that.
    ///
    /// NOTE: this isn't some idiotic premature ejoptimization, there are a
    /// handful of places that call sync_model::save_model() with the user
    /// object locked, and save_model() needs to be able to get to the user
    /// id without locking the user object.
    pub user_id: RwLock<Option<String>>,
    /// Holds the user's data profile (keychain, boards, notes, etc, etc, etc)
    pub profile: RwLock<Profile>,
    /// Need to do some CPU-intensive work and have a Future finished when it's
    /// done? Send it here! Great for decrypting models.
    pub work: Thredder,
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
    pub db: Arc<Mutex<Option<Storage>>>,
    /// Our external API object. Note that most things API-related go through
    /// the Sync system, but there are a handful of operations that Sync doesn't
    /// handle that need API access (invites come to mind). Use sparingly.
    pub api: Arc<Api>,
    /// Holds our heroic search object, used to index/find our notes once the
    /// profile is loaded.
    pub search: Mutex<Option<Search>>,
    /// Sync system configuration (shared state with the sync system).
    pub sync_config: Arc<RwLock<SyncConfig>>,
    /// Holds our sync state data
    sync_state: Arc<RwLock<Option<SyncState>>>,
    /// A lock that keeps our incoming sync from running when we don't want it
    /// to (like while sync is initting and wehaven't loaded our profile yet).
    /// Used alongside Turtl.sync_config.incoming_sync.
    pub incoming_sync_lock: Mutex<()>,
    /// Whether or not we're connected to the API
    pub connected: RwLock<bool>,
}

impl Turtl {
    /// Create a new Turtl app
    pub fn new() -> TResult<Turtl> {
        let num_workers = num_cpus::get() - 1;

        let api = Arc::new(Api::new());
        let kv = Arc::new(RwLock::new(Turtl::open_kv()?));

        // make sure we have a client id
        storage::setup_client_id(kv.clone())?;

        let turtl = Turtl {
            user: RwLock::new(User::default()),
            user_id: RwLock::new(None),
            profile: RwLock::new(Profile::new()),
            api: api,
            msg: Messenger::new(),
            work: Thredder::new("work", num_workers as u32),
            kv: kv,
            db: Arc::new(Mutex::new(None)),
            search: Mutex::new(None),
            sync_config: Arc::new(RwLock::new(SyncConfig::new())),
            sync_state: Arc::new(RwLock::new(None)),
            connected: RwLock::new(false),
            incoming_sync_lock: Mutex::new(()),
        };
        Ok(turtl)
    }

    /// Create/open a new KV store connection
    pub fn open_kv() -> TResult<Storage> {
        let kv_location = storage::db_location(&String::from("turtl-kv"))?;
        Ok(Storage::new(&kv_location, json!({}))?)
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
        let reqres_append_mid: bool = config::get(&["messaging", "reqres_append_mid"])?;
        if reqres_append_mid {
            let res = Response::new(0, data);
            let msg = jedi::stringify(&res)?;
            self.remote_send(Some(mid.clone()), msg)
        } else {
            let res = Response::new_w_id(mid.clone(), 0, data);
            let msg = jedi::stringify(&res)?;
            self.remote_send(None, msg)
        }
    }

    /// Send an error response to a remote request
    pub fn msg_error(&self, mid: &String, err: &TError) -> TResult<()> {
        let reqres_append_mid: bool = config::get(&["messaging", "reqres_append_mid"])?;
        let mut errval = util::json_or_string(format!("{}", err));
        let wrapped = match jedi::get_opt::<bool>(&["wrapped"], &errval) {
            Some(x) => x,
            None => false,
        };
        let wrap_errors: bool = match config::get(&["wrap_errors"]) {
            Ok(x) => x,
            Err(_) => false,
        };
        if !wrap_errors && wrapped {
            errval = jedi::get(&["err"], &errval)?;
        }
        if reqres_append_mid {
            let res = Response::new(1, errval);
            let msg = jedi::stringify(&res)?;
            self.remote_send(Some(mid.clone()), msg)
        } else {
            let res = Response::new_w_id(mid.clone(), 1, errval);
            let msg = jedi::stringify(&res)?;
            self.remote_send(None, msg)
        }
    }

    /// If the `turtl.user` object has a valid ID, set it into `turtl.user_id`
    fn set_user_id(&self) {
        let user_guard = lockr!(self.user);
        match user_guard.id() {
            Some(id) => {
                let mut isengard = lockw!(self.user_id);
                *isengard = Some(id.clone());

                let mut sync_config_guard = lockw!(self.sync_config);
                sync_config_guard.user_id = Some(id.clone());
                drop(sync_config_guard);
            }
            None => {}
        }
    }

    /// Clear out `turtl.user_id` to be None
    fn clear_user_id(&self) {
        let mut isengard = lockw!(self.user_id);
        *isengard = None;

        let mut sync_config_guard = lockw!(self.sync_config);
        sync_config_guard.user_id = None;
        drop(sync_config_guard);
    }

    /// Grab the current user id OR ELSE
    pub fn user_id(&self) -> TResult<String> {
        let isengard = lockr!(self.user_id);
        match isengard.as_ref() {
            Some(x) => Ok(x.clone()),
            None => return TErr!(TError::MissingField(String::from("Turtl.user_id"))),
        }
    }

    /// Call me after a user logs in
    fn post_login(&self) -> TResult<()> {
        self.set_user_id();
        let db = self.create_user_db()?;
        let mut db_guard = lock!(self.db);
        *db_guard = Some(db);
        drop(db_guard);
        User::ensure_keypair(self)?;
        messaging::ui_event("user:login", &Value::Null)?;
        Ok(())
    }

    /// Log a user in
    pub fn login(&self, username: String, password: String) -> TResult<()> {
        User::login(self, username, password, user::CURRENT_AUTH_VERSION)?;
        self.post_login()
    }

    /// Log a user in using a login token
    pub fn login_token(&self, token: String) -> TResult<()> {
        User::login_token(self, token)?;
        self.post_login()
    }

    /// DO Create a new user account
    fn do_join(&self, username: String, password: String, migrate_data: Option<MigrateResult>) -> TResult<()> {
        User::join(self, username, password)?;
        self.set_user_id();
        let db = self.create_user_db()?;
        let mut db_guard = lock!(self.db);
        *db_guard = Some(db);
        drop(db_guard);
        User::post_join(self, migrate_data)?;
        messaging::ui_event("user:login", &Value::Null)?;
        Ok(())
    }

    /// Create a new user account
    pub fn join(&self, username: String, password: String) -> TResult<()> {
        self.do_join(username, password, None)
    }

    /// Create a new user account by migrating from a v0.6 server.
    pub fn join_migrate(&self, old_username: String, old_password: String, new_username: String, new_password: String) -> TResult<()> {
        let login = migrate::check_login(&old_username, &old_password)?;
        if login.is_none() {
            return TErr!(TError::PermissionDenied(String::from("login on old server failed")));
        }
        let migrate_data = migrate::migrate(login.expect("turtl.join_migrate() -- login is None"), |ev, args| {
            debug!("turtl.join_migrate() -- migration event: {}", ev);
            match messaging::ui_event("migration-event", &json!({"event": ev, "args": args})) {
                Ok(_) => {}
                Err(e) => {
                    warn!("turtl.join_migrate() -- error sending migration event: {} / {}", ev, e);
                }
            }
        })?;
        self.do_join(new_username, new_password, Some(migrate_data))
    }

    /// Log a user out
    pub fn logout(&self) -> TResult<()> {
        {
            let mut profile_guard = lockw!(self.profile);
            profile_guard.wipe();
            *profile_guard = Profile::new();
        }
        self.sync_shutdown(false)?;
        self.close_user_db()?;
        self.close_search();
        self.clear_user_id();
        User::logout(self)?;
        {
            let mut userguard = lockw!(self.user);
            *userguard = User::default();
        }
        {
            let mut connguard = lockw!(self.connected);
            *connguard = false;
        }
        messaging::ui_event("user:logout", &Value::Null)?;
        Ok(())
    }

    /// Change the current user's username/password
    pub fn change_user_password(&self, current_username: String, current_password: String, new_username: String, new_password: String) -> TResult<()> {
        self.assert_connected()?;
        {
            let mut user_guard = lockw!(self.user);
            user_guard.change_password(self, current_username, current_password, new_username, new_password)?;
        }
        // all the local data is WRONG. clear it out, after shutting down sync.
        self.sync_shutdown(true)?;
        self.wipe_user_data()?;
        Ok(())
    }

    /// Delete the current user's account (if they are logged in derr)
    pub fn delete_account(&self) -> TResult<()> {
        self.assert_connected()?;
        User::delete_account(self)?;
        self.wipe_user_data()?;
        Ok(())
    }

    /// Returns an Err value if we aren't connected.
    pub fn assert_connected(&self) -> TResult<()> {
        if !(*lockr!(self.connected)) {
            TErr!(TError::ConnectionRequired)
        } else {
            Ok(())
        }
    }

    /// Poll `turtl.db` until either it exists or a few seconds have passed.
    fn check_db_exists(&self) -> TResult<()> {
        let exists = {
            let db_guard = lock!(self.db);
            db_guard.is_some()
        };
        if !exists {
            for _i in 0..5 {
                let exists = {
                    let db_guard = lock!(self.db);
                    db_guard.is_some()
                };
                if exists { break; }
                info!("turtl.check_db_exists() -- waiting on `turtl.db`...");
                util::sleep(1000);
            }
        }
        let exists = {
            let db_guard = lock!(self.db);
            db_guard.is_some()
        };
        if !exists {
            return TErr!(TError::MissingField(String::from("Turtl.db")));
        }
        Ok(())
    }

    /// Start our sync system. This should happen after a user is logged in, and
    /// we definitely have a Turtl.db object available.
    pub fn sync_start(&self) -> TResult<()> {
        // it's possible that login/join return before the db is initialized. in
        // that case, we wait for it here, up to 5s. if after then we don't have
        // our heroic db, error out ='[
        self.check_db_exists()?;

        // increment our run version to catch rogue sync threads
        {
            let mut sync_config_guard = lockw!(self.sync_config);
            sync_config_guard.run_version += 1;
        }

        // lock down incoming syncs so we have a chance to load our profile
        // before dealing with a bunch of sync records
        let sync_lock = self.incoming_sync_lock.lock();
        // start the sync, and save the resulting state into Turtl
        let sync_state = sync::start(self.sync_config.clone(), self.api.clone(), self.db.clone())?;
        {
            let mut state_guard = lockw!(self.sync_state);
            *state_guard = Some(sync_state);
        }

        self.load_profile()?;
        messaging::ui_event("profile:loaded", &())?;
        self.index_notes()?;
        messaging::ui_event("profile:indexed", &())?;

        // wipe our incoming sync queue. we're about to synchronize all our
        // in-mem state with what's in the DB, so we don't really need to run
        // MemorySaver on the incoming syncs we just created while doing our
        // sync init.
        {
            let sync_config_guard = lockr!(self.sync_config);
            loop {
                if sync_config_guard.incoming_sync.try_pop().is_none() { break; }
            }
        }
        // let your freak flag fly, incoming syncs
        drop(sync_lock);

        Ok(())
    }

    /// Shut down the sync system
    pub fn sync_shutdown(&self, join: bool) -> TResult<()> {
        let mut guard = lockw!(self.sync_state);
        info!("turtl.sync_shutdown() -- has state? {}", guard.is_some());
        if guard.is_none() { return Ok(()); }
        {
            let state = guard.as_mut().expect("turtl::Turtl.sync_shutdown() -- sync_state is None");
            (state.shutdown)();
            if join {
                info!("turtl.sync_shutdown() -- waiting on {} handles", state.join_handles.len());
                loop {
                    let hn = state.join_handles.pop();
                    match hn {
                        Some(x) => match x.join() {
                            Ok(_) => (),
                            Err(e) => error!("turtl::sync_shutdown() -- problem joining thread: {:?}", e),
                        },
                        None => break,
                    }
                }
            }
        }
        *guard = None;

        // set connected to false on sync shutdown
        let mut connguard = lockw!(self.connected);
        *connguard = false;
        Ok(())
    }

    /// Pause the sync system (if active)
    pub fn sync_pause(&self) {
        let guard = lockr!(self.sync_state);
        if guard.is_some() { (guard.as_ref().expect("turtl::Turtl.sync_pause() -- sync_state is None").pause)(); }
    }

    /// Resume the sync system (if active)
    pub fn sync_resume(&self) {
        let guard = lockr!(self.sync_state);
        if guard.is_some() { (guard.as_ref().expect("turtl::Turtl.sync_resume() -- sync_state is None").resume)(); }
    }

    /// Returns whether or not the sync system is running
    pub fn sync_running(&self) -> bool {
        let guard = lockr!(self.sync_state);
        if guard.is_some() {
            (guard.as_ref().expect("turtl::Turtl::sync_running() -- sync_state is None").enabled)()
        } else {
            false
        }
    }

    /// Returns whether or not syncing has been initialized (ie, sync_start has
    /// been called). Basically just tests for the presence of sync_state.
    pub fn sync_ready(&self) -> bool {
        let guard = lockr!(self.sync_state);
        guard.is_some()
    }

    /// Create a new per-user database for the current user.
    pub fn create_user_db(&self) -> TResult<Storage> {
        let user_id = self.user_id()?;
        let db_location = self.get_user_db_location(&user_id)?;
        let dumpy_schema = schema::get_schema();
        Storage::new(&db_location, dumpy_schema)
    }

    /// Close the per-user database.
    pub fn close_user_db(&self) -> TResult<()> {
        let mut db_guard = lock!(self.db);
        if let Some(db) = db_guard.as_mut() {
            db.close()?;
        }
        *db_guard = None;
        Ok(())
    }

    /// Shut down the search system
    pub fn close_search(&self) {
        let mut search_guard = lock!(self.search);
        *search_guard = None;
    }

    /// Get the physical location of the per-user database file we will use for
    /// the current logged-in user.
    pub fn get_user_db_location(&self, user_id: &String) -> TResult<String> {
        lazy_static! {
            static ref RE_API_FORMAT: Regex = Regex::new(r"(?i)[^a-z0-9]").expect("turtl::Turtl.get_user_db_location() -- failed to compile regex");
        }
        let api_endpoint = config::get::<String>(&["api", "endpoint"])?;
        let server = RE_API_FORMAT.replace_all(&api_endpoint, "");
        let user_db = format!("turtl-user-{}-srv-{}", user_id, server);
        storage::db_location(&user_db)
    }

    /// Given a model that we suspect we have a key entry for, find that model's
    /// key, set it into the model, and return a reference to the key.
    /// TODO: move this to the protected model, duhh
    pub fn find_model_key<T>(&self, model: &mut T) -> TResult<()>
        where T: Protected + Keyfinder
    {
        // check if we have a key already. if you're trying to re-find the key,
        // make sure you model.set_key(None) before calling...
        if model.key().is_some() { return Ok(()); }

        let notfound = TErr!(TError::NotFound(format!("key for `{}` not found ({:?})", model.model_type(), model.id())));

        /// A standard "found a key" function
        fn found_key<T>(model: &mut T, key: Key) -> TResult<()>
            where T: Protected
        {
            model.set_key(Some(key));
            return Ok(());
        }

        // the user object is encrypted with the master key.
        //
        // keychain entries are always encrypted using the user's key, so we
        // skip the song and dance of searching and just set it in here.
        if (model.model_type() == "user" && model.id_or_else()? == self.user_id()?) || model.model_type() == "keychain" {
            let user_key = {
                let user_guard = lockr!(self.user);
                user_guard.key_or_else()?
            };
            return found_key(model, user_key);
        }

        // fyi ders, this read lock is going to be open until we return
        let profile_guard = lockr!(self.profile);
        let ref keychain = profile_guard.keychain;

        // check the keychain right off the bat. it's quick and easy.
        if model.id().is_some() {
            match keychain.find_key(model.id().expect("turtl::Turtl.find_model_key() -- model.id() is None")) {
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
        let mut search = model.get_key_search(self)?;

        // push the user's key into our search, if it's available
        {
            let user_guard = lockr!(self.user);
            if user_guard.id().is_some() && user_guard.key().is_some() {
                let id = user_guard.id().expect("turtl::Turtl.find_model_key() -- user.id() is None").clone();
                let key = user_guard.key().expect("turtl::Turtl.find_model_key() -- user.key() is None").clone();
                drop(user_guard);
                search.upsert_key(self, &id, &key, &String::from("user"))?;
            }
        }

        let def = Vec::new();
        let mut encrypted_keys = Vec::new();
        for enckey in model.get_keys().unwrap_or(&def) {
            encrypted_keys.push(enckey.clone());
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
        // grab the model.keys collection.
        for keyref in encrypted_keys {
            let ref encrypted_key = keyref.k;
            let ref object_id = keyref.id;

            // check if this object is in the keychain first. if so, we can use
            // its key to decrypt our encrypted key
            match keychain.find_key(object_id) {
                Some(decrypting_key) => {
                    match protected::decrypt_key(&decrypting_key, encrypted_key) {
                        Ok(key) => return found_key(model, key),
                        Err(e) => {
                            warn!("turtl.find_model_key() -- found keychain entry for model {:?} (via item {}) but could not decrypt it: {}", model.id(), object_id, e);
                        }
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
                    Err(e) => {
                        warn!("turtl.find_model_key() -- found key for model {:?} (via item {}) but could not decrypt it: {}", model.id(), object_id, e);
                    }
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
                    warn!("turtl.find_models_keys() -- skipping model {:?}/{}: problem finding key", model.id(), model.model_type());
                    errcount += 1;
                },
            }
        }
        if errcount > 0 {
            warn!("turtl.find_models_keys() -- load summary: couldn't load keys for {} models", errcount);
        }
        Ok(())
    }

    /// Load the profile from disk.
    ///
    /// Meaning, we decrypt the keychain, spaces, and boards and store them
    /// in-memory in our `turtl.profile` object.
    pub fn load_profile(&self) -> TResult<()> {
        let db_guard = lock!(self.db);
        if db_guard.is_none() {
            return TErr!(TError::MissingField(String::from("Turtl.db")));
        }
        let db = db_guard.as_ref().expect("turtl::Turtl.load_profile() -- db is None");
        let mut keychain: Vec<KeychainEntry> = db.all("keychain")?;
        let mut spaces: Vec<Space> = db.all("spaces")?;
        let mut boards: Vec<Board> = db.all("boards")?;
        let invites: Vec<Invite> = db.all("invites")?;

        // decrypt the keychain
        self.find_models_keys(&mut keychain)?;
        let keychain: Vec<KeychainEntry> = protected::map_deserialize(self, keychain)?;
        let mut sync_item = SyncRecord::default();
        sync_item.action = SyncAction::Add;
        for entry in keychain {
            entry.mem_update(self, &mut sync_item)?;
        }

        // now decrypt the spaces
        self.find_models_keys(&mut spaces)?;
        let spaces: Vec<Space> = protected::map_deserialize(self, spaces)?;
        for space in spaces {
            space.mem_update(self, &mut sync_item)?;
        }

        // now decrypt the boards
        self.find_models_keys(&mut boards)?;
        let boards: Vec<Board> = protected::map_deserialize(self, boards)?;
        for board in boards {
            board.mem_update(self, &mut sync_item)?;
        }

        // invites are NOT decrypted. they are stored as-is.
        // set the invites into the profile
        for invite in invites {
            invite.mem_update(self, &mut sync_item)?;
        }

        let mut user_guard = lockw!(self.user);
        user_guard.deserialize()?;
        Ok(())
    }

    /// Load/deserialize a set of notes by id.
    pub fn load_notes(&self, note_ids: &Vec<String>) -> TResult<Vec<Note>> {
        let db_guard = lock!(self.db);
        let db = match (*db_guard).as_ref() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(String::from("Turtl.db"))),
        };

        let notes: Vec<Note> = db.by_id("notes", note_ids)?;
        // make sure notes are ordered based on the ids we passed
        let mut notes = {
            let mut tmp = Vec::with_capacity(notes.len());
            let mut sort_hash: HashMap<String, Note> = HashMap::with_capacity(notes.len());
            for note in notes {
                sort_hash.insert(note.id().expect("turtl::Turtl.load_notes() -- note.id() is None").clone(), note);
            }
            for note_id in note_ids {
                if let Some(note) = sort_hash.remove(note_id) {
                    tmp.push(note);
                }
            }
            tmp
        };
        self.find_models_keys(&mut notes)?;
        protected::map_deserialize(self, notes)
    }

    /// Take all the (encrypted) notes in our profile data then decrypt, index,
    /// and free them. The idea is we can get a set of note IDs from a search,
    /// but we're not holding all our notes decrypted in memory at all times.
    pub fn index_notes(&self) -> TResult<()> {
        let db_guard = lock!(self.db);
        if db_guard.is_none() {
            return TErr!(TError::MissingData(String::from("Turtl.db")));
        }
        let db = db_guard.as_ref().expect("turtl::Turtl::index_notes() -- db is None");
        let mut notes: Vec<Note> = db.all("notes")?;
        self.find_models_keys(&mut notes)?;
        let notes: Vec<Note> = protected::map_deserialize(self, notes)
            .or_else(|e| -> TResult<Vec<Note>> {
                error!("turtl.index_notes() -- there was a problem indexing notes: {}", e);
                Err(e)
            })?;
        let mut search = Search::new()?;
        for note in &notes {
            match search.index_note(note) {
                Ok(_) => {},
                // keep going on error
                Err(e) => error!("turtl.index_notes() -- problem indexing note {:?}: {}", note.id(), e),
            }
        }
        let mut search_guard = lock!(self.search);
        *search_guard = Some(search);
        Ok(())
    }

    /// Log out the current user (if logged in) and wipe ALL local SQL databases
    /// from our data folder.
    pub fn wipe_app_data(&self) -> TResult<()> {
        self.sync_shutdown(false)?;
        util::sleep(5000);
        self.logout()?;

        let mut kv_guard = lockw!(self.kv);
        kv_guard.close()?;
        let data_folder = data_folder()?;
        debug!("turtl.wipe_app_data() -- wiping everything in {}", data_folder);
        let paths = fs::read_dir(data_folder)?;
        // wipe all databases
        for entry in paths {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() { continue; }
            let filename = entry.file_name();
            let filename_str = match filename.to_str() {
                Some(x) => x,
                None => return TErr!(TError::Msg(format!("error converting OsString into &str"))),
            };
            if &filename_str[0..6] != "turtl-" { continue; }
            fs::remove_file(&path)?;
            info!("turtl.wipe_app_data() -- removing {}", path.display());
        }

        // wipe all note files
        let files = FileData::file_finder_all(None, None)?;
        for file in files {
            fs::remove_file(&file)?;
            info!("turtl.wipe_app_data() -- removing {}", file.display());
        }

        (*kv_guard) = Turtl::open_kv()?;
        Ok(())
    }

    /// Wipe any local database(s) for the current user (and log them out)
    pub fn wipe_user_data(&self) -> TResult<()> {
        let user_id = self.user_id()?;
        self.sync_shutdown(false)?;
        util::sleep(5000);
        self.logout()?;

        let db_loc = self.get_user_db_location(&user_id)?;
        if db_loc != ":memory:" {
            info!("turtl.wipe_user_data() -- removing {}", db_loc);
            fs::remove_file(&db_loc)?;
        }

        let files = FileData::file_finder_all(Some(&user_id), None)?;
        for file in files {
            fs::remove_file(&file)?;
            info!("turtl.wipe_user_data() -- removing {}", file.display());
        }

        Ok(())
    }

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) -> TResult<()> {
        self.sync_shutdown(false)?;
        self.logout()?;
        Ok(())
    }
}

// Probably don't need this since `shutdown` just wipes our internal state which
// would happen anyway if Turtl is dropped, but whatever.
impl Drop for Turtl {
    fn drop(&mut self) {
        match self.shutdown() {
            Err(e) => error!("Turt::drop() -- error shutting down Turtl: {}", e),
            _ => (),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use ::std::sync::{RwLock, Mutex};

    use ::jedi;

    use ::crypto::{self, Key};
    use ::search::Query;
    use ::models::model::Model;
    use ::models::protected::Protected;
    use ::models::keychain::KeychainEntry;
    use ::models::user::User;
    use ::models::note::Note;
    use ::models::board::Board;
    use ::models::sync_record::{SyncRecord, SyncAction, SyncType};
    use ::sync::sync_model;

    protected! {
        #[derive(Serialize, Deserialize)]
        pub struct Dog {
            #[protected_field(public)]
            pub user_id: Option<String>,

            #[protected_field(private)]
            pub name: Option<String>,
        }
    }

    /// Give us a new Turtl to start running tests on
    pub fn with_test(logged_in: bool) -> Turtl {
        ::init(String::from("{}")).unwrap();
        let turtl = Turtl::new().unwrap();
        if logged_in {
            let user_key = Key::new(crypto::from_base64(&String::from("jlz71VUIns1xM3Hq0fETZT98dxzhlqUxqb0VXYq1KtQ=")).unwrap());
            let mut user: User = jedi::parse(&String::from(r#"{"id":"51","username":"slippyslappy@turtlapp.com","storage":104857600,"body":"AAYBAAzWT6T3jTOu+I0DN7GKxgMocHTwkFPADW6pogRjUDo="}"#)).unwrap();
            let user_auth = String::from("000601000c9af06607bbb78b0cab4e01f2fda9887cf4fcdcb351527f9a1a134c7c89513241f8fc0d5d71341b46e792242dbce7d43f80e70d1c3c5c836e72b5bd861db35fed19cadf45d565fa95e7a72eb96ef464477271631e9ab375e74aa38fc752a159c768522f6fef1b4d8f1e29fdbcde59d52bfe574f3d600d6619c3609175f29331a353428359bcce95410d6271802275807c2fabd50d0189638afa7ce0a6");
            user.do_login(user_key, user_auth);
            let mut user_guard = lockw!(turtl.user);
            *user_guard = user;
            drop(user_guard);
            turtl.set_user_id();
            let db = turtl.create_user_db().unwrap();
            let mut db_guard = lock!(turtl.db);
            *db_guard = Some(db);
            drop(db_guard);
        }
        turtl
    }

    #[test]
    fn finding_keys() {
        let enc_board = String::from(r#"{"id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"keys":[{"k":"AAYBAAz9znE+csObRfJh7v1+vILRefrGx/ZC97qtGetYvtPYr3gO4v4AnhWPP/z49ESptJ1aSIOWTzPKBt5B1fI=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"body":"AAYBAAxEVD6FeHQaEl9yh3M9LVJTh0poYU8FA1SxwYVn/8N1SBNYBYzuWcfXMoTFrmz0CHum"}"#);
        let enc_note = String::from(r#"{"id":"015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","user_id":51,"file":{},"keys":[{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAzSgseWF4MMXhZ8RDg3igwoghg9vAdlwaG70EwncM9odiZ6rQq5U/Dv1ZXTUgOGolwEGZ7PjFYw8IJhQ10="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAysMP4OBviiXtL86+pmH2jpIYH9D5AsbpLTQ7GoTXsugfvyM3hhJuUNsBQlPVtbOqATaS87Mx3sDnQXEFQ="}],"mod":1498539524,"body":"AAYBAAzH4KVxGsdEq2PhfjX6dTSmPiVye8gv+Yp457UiYEce5jrL6T1K4WyNnvZizqeKOPGyMnqAtBxxNrClfwV4YVdlNDAQQAKQSSln+K0CvSgcIdC8mRCHqOobFWazYy7pS1SlKrNz9tBnJXjvJOzjRjI4GAGVVNj9t2YoJfFFDVFi1slTEC8SRDXj82AvaYIoGjF1bnw0FY4d3AOiigdJa4s5VRbsGG/75djUinn0i1avSqfdm5E="}"#);
        let board_key = Key::new(crypto::from_base64(&String::from("WOaCXXSuzs+8SiljyBdtqXE9XmQJP1uAY9DCK6EC4e8=")).unwrap());
        let note_key = Key::new(crypto::from_base64(&String::from("VAkQBuwoPXAQdDOIHZ/ItNWL0xZh+qBT5GKtj92HZ/8=")).unwrap());
        let mut board: Board = jedi::parse(&enc_board).unwrap();
        let mut note: Note = jedi::parse(&enc_note).unwrap();

        let turtl = with_test(true);

        // add the note's key as a direct entry to the keychain
        let mut profile_guard = lockw!(turtl.profile);
        profile_guard.keychain.upsert_key(&turtl, note.id().unwrap(), &note_key, &String::from("note")).unwrap();
        drop(profile_guard);

        // see if we can find the note as a direct entry
        {
            turtl.find_model_key(&mut note).unwrap();
            let found_key = note.key().unwrap();
            assert_eq!(found_key, &note_key);
        }

        // clear out the keychain, and add the board's key to the keychain
        let mut profile_guard = lockw!(turtl.profile);
        profile_guard.keychain.entries.clear();
        assert_eq!(profile_guard.keychain.entries.len(), 0);
        profile_guard.keychain.upsert_key(&turtl, board.id().unwrap(), &board_key, &String::from("board")).unwrap();
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
        let mut profile_guard = lockw!(turtl.profile);
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
        let turtl = with_test(true);

        // load our profile from a few big JSON blobs. we do this out of scope
        // so's not to be tempted to use them later on...we want the profile to
        // load itself completely from the DB and deserialize successfully w/o
        // having access to any of the data we put in here.
        {
            let mut db_guard = lock!(turtl.db);
            let db = db_guard.as_mut().unwrap();
            let keychain: Vec<KeychainEntry> = jedi::parse(&String::from(r#"[
                {"id":"015bac22440b4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30020","type":"space","item_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"body":"AAYBAAwuE3ASfPUmqgFhjcllp4atv6bJ/hf1CUjfPuMs/g+0nDcrC6Ye6AAr26Gk/0LWwjB0mgT3/Bb/00SxFrM97YDA6EUs1xxNG2SKakMTz585vw=="},
                {"id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30028","type":"space","item_id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","user_id":51,"body":"AAYBAAwl4cOKFgxzAM8CFFCEiy4SKbC01qhtI40O7El7UG05UneASSsxdKN15bFZUAyD0TQPx/fEKf5zn251Bdmdl/mAw0aNKYX9/60/mpj17+6zsw=="},
                {"id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30030","type":"space","item_id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","user_id":51,"body":"AAYBAAwgRYgtpMVP2H66+WJd0BWEuN7Cqoh4TasleTl77Dim4gvOPIjq2pvtse+O0ywW0B98CCoo4wg5JP3UJKpb3On20fgPmx5sgxgSszk3IfU0ow=="},
                {"id":"015bae37fb224944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30048","type":"space","item_id":"015a8362721ab6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90026","user_id":51,"body":"AAYBAAw9t7R3TUCPVCYUyh4mqJK/OMUUJOnYZviN2yCdoD3JUA0c7mjfdmvYAnNmSYMdJgGAbyHYBYqR4KWwIxM9xhk5wAlF59ZKXc7r+ikfCltUYw=="},
                {"id":"015bb25a31c59097895e547164bd5935025703c9d5188f2be449364e26aa736e01a6f74ceaac00f4","type":"space","item_id":"015a836469b5b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90041","user_id":51,"body":"AAYBAAyp/Q4ry3GR1fU7034oy52mIccQ8I7U5ZPyRgFvaAX4QrlVCSa53Z7NLP1iZU7PW3bnV40rKdfGALIJ2zmo8pifWnItV+ChVScz4Hwy5PPTGg=="},
                {"id":"015bffa0d30d950451886efa5af640eda04c689cb9e3de1caea1b59c732b265e8a5aae7c96cd00b9","type":"space","item_id":"015a836469b5b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90041","user_id":51,"body":"AAYBAAwrVM4bcSP3qwkFEv5qAQSq7O5/8qBa2nZFPFQdB5ZGYc7Qa+GKB55H5l2F37s6QSa84n8FX6/tbTmwNqBSyEsNSAI9OHw0SyXUf9dugVzsPQ=="},
                {"id":"015bffa2ce42950451886efa5af640eda04c689cb9e3de1caea1b59c732b265e8a5aae7c96cd00fa","type":"space","item_id":"015a836469b5b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90041","user_id":51,"body":"AAYBAAwT5rBK44hbHZQOICE3FnzOw9B606+4gEl64YCWqqIN7Vwsr6x8Ff9XGvvwbOCJQyeWGM9tpLSIr5uwFuiYe9Mc13h4odPiCudlXXJzcz09Fg=="},
                {"id":"015c14254472cb9346f941d635d7cd81602dee4af381029f25e1d637ebce44b6170700262e0c0081","type":"space","item_id":"015a836469b5b6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c90041","user_id":51,"body":"AAYBAAyCp5xUilhR0WJt5r8IeV/dRvkFsoHjUC8k5Kc9Wy5YqN3+rzM7NHBmwivRNofr2DS22GrdkngVUhEOqjdgoc37djTTRumZDRjAwsw0f9Wpfg=="}
            ]"#)).unwrap();
            let spaces: Vec<Space> = jedi::parse(&String::from(r#"[{"id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"members":[{"id":90,"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":"51","role":"owner","created":"2017-04-26T19:19:40.782Z","updated":"2017-04-26T19:19:40.782Z","username":"andrew@lyonbros.com"}],"invites":[],"keys":[],"body":"AAYBAAzSnOFnsF8LgCqQbQDhjowbdfeuWYRfRevmY/ie0GOeJhEbxaaloFsT7wblZjYMGd+ocL0TKvUYqO0U/qJEwFM2Uh0="},{"id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","user_id":51,"members":[{"id":91,"space_id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","user_id":"51","role":"owner","created":"2017-04-26T19:19:40.819Z","updated":"2017-04-26T19:19:40.819Z","username":"andrew@lyonbros.com"}],"invites":[],"keys":[],"body":"AAYBAAweqy/gx9HTkvkQXNgCvSyVApuJKXEOJNTqlEU8udrW2qb5/gzjdn5dOh/fCI9HHUzyYysiFG739oLBTexxdg=="},{"id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","user_id":51,"members":[{"id":92,"space_id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","user_id":"51","role":"owner","created":"2017-04-26T19:19:40.871Z","updated":"2017-04-26T19:19:40.871Z","username":"andrew@lyonbros.com"}],"invites":[],"keys":[],"body":"AAYBAAzAf7huCdGhIfZ8LakfUsgOLXsKxsMkK1ulo7G+lTEElums6ViZ8SLxFUnF3kn1BDtbN29sT8NAxCDlKskFkg=="}]"#)).unwrap();
            let boards: Vec<Board> = jedi::parse(&String::from(r#"[{"id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"keys":[{"k":"AAYBAAz9znE+csObRfJh7v1+vILRefrGx/ZC97qtGetYvtPYr3gO4v4AnhWPP/z49ESptJ1aSIOWTzPKBt5B1fI=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"body":"AAYBAAxEVD6FeHQaEl9yh3M9LVJTh0poYU8FA1SxwYVn/8N1SBNYBYzuWcfXMoTFrmz0CHum"},{"id":"015bac2244f54944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30039","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"keys":[{"k":"AAYBAAwYIRhvHsm43RJnwXRXCIGnzCs2e+eBW6Wzyr+ojo00PY123AmMGMSqq6IStUrSbteDdxG4iRZEyBJwY8g=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"body":"AAYBAAyijrKoEz3RznfMJslv2qO17BqmiYP8SgDW1i/AkC/O50Y6jizxq2cljfwlwRn0"},{"id":"015bac2245044944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3003e","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","user_id":51,"keys":[{"k":"AAYBAAyr/nHZWKTTcsvdokRUReLnUDM60/D0BigVJ1sRcGZg3cQ1M0ocZzC45nehLqw5iJAO/N2RP/AKoKOqTio=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"body":"AAYBAAzXFMlrh9a4hWujp2PKTeXoSMbfSwX2YpPgC6mg2IoUSfO4/NU8bbVQNTV+pG64PRBz"}]"#)).unwrap();
            let notes: Vec<Note> = jedi::parse(&String::from(r#"[
                {"id":"015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e","mod":1497592545,"body":"AAYBAAzChZjAOGoAQ0MMjLofXXHarNfUu9Eqlv/063dUH4kbrp8Mnmw+XIn7LxAHloxdMpdiVDz5SAcLyy5DftjOjEEwKfylexz+C9zq5CQSjsQzuRQYMxD7TAwiJZLd+CsM1msek0kkhIB2whG6plMC8Hlyu1bMdcvWJ3B7Oonp89V57ycedVsSMWE28ablc3X3aKO8LRjCnoZlOK/UbZZYQnkm4roGV8dWlbKziTHm8R9ctBrxceo5ky3molooQ6GPKIPbm+lomsyrGDBG4DBDd7KlMJ1LCcsXzYWLnqvQyYny2ly37l5x3Y4dOcZVZ0gxkSzvHe37AzQl","keys":[{"k":"AAYBAAzuWB81LF46TLQ0b9aibwlL4lT5FTxw1UNxtUNKA2zuzW91drujc53uMQipFhcq6s6Ff9mDQr0Ew5H7Guw=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"user_id":51,"board_id":null,"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"},
                {"body":"AAYBAAxn4lZjNtOesNLrTWFlvtU46XFdtlhkmw24SbWWWc7YlLYAfFfYBcTf9hAqzeTqWgiOM48Ln5+Iy6dnzBWWGoThV46arfPYBRwxXGpGLm/GRKnBpSwoGIJHc6l4pEgpUYn0ozU6hvxrLiFzTebWPZDGFQqJIT2uRC3Frzs/gtpn3cyec/alRtywrWzkRhIyXagHAoEI1sbGatojXD6f3YVQD9ZmF6qWvLNDTuP/qaCM/eM2Um/Nwzg=","id":"015caf7c5f4d2af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a022b","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","user_id":51,"keys":[{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAzbm2cMWZVRzKHwY34FmdBwOpyzTusHMLbb07K60bvi8FaK7QaVzx1GQlds8R/qlb5shkaTw+7nhTTB6sA="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAzKi0OOAhS4eNCKJdeVt+P//PERRjgaMOBOdCxqfk9NIe6TEQW1Arx40c4YuVInAi9HhHTtuaWZ+rzydDk="}],"mod":1497592783},
                {"body":"AAYBAAxc9IwivKewKefuur8wn/FNbzic0iumwistONOnzewNQLrkdNoAEzeVT4dHc7oDlwrks4XZ3p4k3rkYUVjBf7qvAysN66iriXZwATU4YggKN1jCl+SvseuXBaRd1dkhADiQ7FJ8p9v7CMI3acA+13J66D/l5TuN01tQU6bHtYghwGWzOw8rMRAYNBait36+3C0iKLeJ8TStROS6cM1SlrvjudaC1uv9YVkYdfdHGDi1kOWGtwb/N4XZi70=","id":"015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","user_id":51,"keys":[{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAznB2cCD5W7QiOmu83X1jLukrnwfONjTEhHF+V/IdHb4mAKGXisF3Sxy62kYP+4kC7Zi1vqQHTpGQ4ZPjM="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAwaJPw5RwH4vdAFFX2Cdwj/s6p4CKjhm2EtpmBsqKVDDP+gBGD1YOnWvXh/48WfCBY4uSQwPREadTjXhuA="}],"mod":1498539524},
                {"body":"AAYBAAzn9I3n6W0LvYaek6b0a9DODt4OW2iBh6/FIRDjinOCjGCLy+Dmd6rGJ/nvQiBt16Gfz69UKtcEn5DG3yULKEi+1AOx8dL5nVa7Of6UCFMM65gFNso/pGXuQHzUO/yaIdzY/A36YExC+ZDmd+ypmDnnlOggfUv48YeENpIrTuWqKl0r1tW4kZtSGNLsLZLqiii5YP3eDiqY6U1MaM8A6vAwXvSfgBCuQmcDel1tTIkhKs1E19VemWSy4TV0Rp46M9cQ/hTIQOc0nFPZOZMpXq6AIj8gxjpwKdMk6sRLq5L2CjEQxN/RnAcry1pPlNMfhmSxYfZRvTbHbzgJ6XGsGNddCaRDZe+AWECqM4qCthaSW0buUL6U03cez1uW3poWjyNqPfZex4gI","id":"015d0aee51102af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a0249","space_id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","board_id":null,"user_id":51,"keys":[{"s":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","k":"AAYBAAyKhhYKuPhgjwa8F6HDpCZf6ICXguP4UigRSdqA4mMfn6rbMIa7S6UxTO2YM074fPSVK1+thohhyqlUMqY="}],"mod":1499126977},
                {"file":{"body":"AAYBAAzodtG8+Uq9olvLXFY0bwdx37JhXJpLjVUHxDpTkwW2Ox7IXv9bJB0Zw6h1UNeoCjElX0eF4PjtKBt6ORyR+7e1vH7WCvDBeJXEHobyUPBRcFMEWbVIQpNl2RDtzQOLqjqj10O2B19K4Ujw6Z95gCL+0jIBE4L9PVXcQyN2Il5Po3i1NE6GR/lRSz8=","id":"015d0b84f5562af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00f5","size":174295,"has_data":2},"body":"AAYBAAxJgxn8iDMPMG/wE5q0V+NfIDfcCPv7rbijkovRC0rHQj81fjdDGCkG1RKKvTnUY2cqJ1ifGIAU9/DNwY3XtCQFEu29mdQIvAOBAOsv4t6OWR85JMCoTPbH3PBOlWpFiGiYr6u0zpMQD80vhmgXYCIpjz4p8nySgNQ9NWsLfDJQzzrAElp1na/fFt+YBhRF3CTY7l+8CL3uvOs=","id":"015d0b84f5562af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00f5","space_id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","board_id":null,"user_id":51,"has_file":true,"keys":[{"s":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","k":"AAYBAAy5kqVvdFgzGatNxyKDvi7+vXXfmKxEIvX3w5UdVO4H4xo45K/IOYyXqIx64d9p/kWos/RA9/kVlaDGPak="}],"mod":1499136849}
            ]"#)).unwrap();

            for entry in &keychain { db.save(entry).unwrap(); }
            for space in &spaces { db.save(space).unwrap(); }
            for board in &boards { db.save(board).unwrap(); }
            for note in &notes { db.save(note).unwrap(); }
        }

        turtl.load_profile().unwrap();
        let profile_guard = lockr!(turtl.profile);
        assert_eq!(profile_guard.keychain.entries.len(), 5);
        assert_eq!(profile_guard.spaces.len(), 3);
        assert_eq!(profile_guard.boards.len(), 3);
        assert_eq!(profile_guard.boards[0].title.as_ref().unwrap(), &String::from("Bookmarks"));
        turtl.index_notes().unwrap();

        fn parserrr(json: &str) -> Query {
            jedi::parse(&String::from(json)).unwrap()
        }

        let search_guard = lock!(turtl.search);
        let search = search_guard.as_ref().unwrap();

        // this stuff is mostly covered in the search tests, but let's
        // just make sure here.

        let qry = parserrr(r#"{"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","boards":["015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034"]}"#);
        assert_eq!(search.find(&qry).unwrap().0, vec![String::from("015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa"), String::from("015caf7c5f4d2af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a022b")]);

        let qry = parserrr(r#"{"space_id":"015bac2244d44944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3002e","text":"grandpa happy"}"#);
        assert_eq!(search.find(&qry).unwrap().0, vec![String::from("015d0b84f5562af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00f5")]);

        let qry = parserrr(r#"{"space_id":"015bac2244c84944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30026","text":"grandpa happy"}"#);
        let (notes, total) = search.find(&qry).unwrap();
        assert_eq!(notes.len(), 0);
        assert_eq!(total, 0);

        let qry = parserrr(r#"{"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","boards":[]}"#);
        assert_eq!(
            search.find_tags(&qry).unwrap(),
            vec![
                (String::from("fuck yeah"), 2),
                (String::from("america"), 1),
                (String::from("cult"), 1),
                (String::from("free market"), 1),
            ]
        );

        // make sure loading notes by id preserves order
        let note_ids = vec![
            String::from("015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e"),
            String::from("015caf7c5f4d2af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a022b"),
            String::from("015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa"),
            String::from("015d0aee51102af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a0249"),
            String::from("015d0b84f5562af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00f5"),
        ];
        let notes = turtl.load_notes(&note_ids).unwrap();
        let grabbed_ids = notes.into_iter().map(|x| x.id().unwrap().clone()).collect::<Vec<_>>();
        assert_eq!(grabbed_ids, note_ids);

        let note_ids = vec![
            String::from("015d0aee51102af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a0249"),
            String::from("015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e"),
            String::from("015d0b84f5562af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00f5"),
            String::from("015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa"),
            String::from("015caf7c5f4d2af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a022b"),
        ];
        let notes = turtl.load_notes(&note_ids).unwrap();
        let grabbed_ids = notes.into_iter().map(|x| x.id().unwrap().clone()).collect::<Vec<_>>();
        assert_eq!(grabbed_ids, note_ids);
    }

    #[test]
    fn stores_models() {
        let user_key = Key::new(crypto::from_base64(&String::from("jlz71VUIns1xM3Hq0fETZT98dxzhlqUxqb0VXYq1KtQ=")).unwrap());
        let mut user: User = jedi::parse(&String::from(r#"{"id":"51","username":"slippyslappy@turtlapp.com","storage":104857600}"#)).unwrap();
        let user_auth = String::from("000601000c9af06607bbb78b0cab4e01f2fda9887cf4fcdcb351527f9a1a134c7c89513241f8fc0d5d71341b46e792242dbce7d43f80e70d1c3c5c836e72b5bd861db35fed19cadf45d565fa95e7a72eb96ef464477271631e9ab375e74aa38fc752a159c768522f6fef1b4d8f1e29fdbcde59d52bfe574f3d600d6619c3609175f29331a353428359bcce95410d6271802275807c2fabd50d0189638afa7ce0a6");
        user.do_login(user_key, user_auth);

        let mut turtl = with_test(false);
        turtl.user = RwLock::new(user);
        {
            let user_guard = lockr!(turtl.user);
            let mut isengard = lockw!(turtl.user_id);
            *isengard = Some(user_guard.id().unwrap().clone());
        }

        let db = turtl.create_user_db().unwrap();
        turtl.db = Arc::new(Mutex::new(Some(db)));

        let mut space: Space = jedi::parse(&String::from(r#"{
            "user_id":69,
            "title":"get a job"
        }"#)).unwrap();
        // save our space to "disk"
        let space_val: Value = sync_model::save_model(SyncAction::Add, &turtl, &mut space, false).unwrap();
        let mut note: Note = jedi::parse(&String::from(r#"{
            "user_id":69,
            "space_id":"8884442",
            "board_id":null,
            "type":"bookmark",
            "title":"my fav website LOL",
            "tags":["website","bookmark","naturefresh milk"],
            "url":"https://yvettesbridalformal.p1r8.net/",
            "text":"v8!"
        }"#)).unwrap();
        let space_id: String = jedi::get(&["id"], &space_val).unwrap();
        note.space_id = space_id.clone();
        // save our note to "disk"
        let val: Value = sync_model::save_model(SyncAction::Add, &turtl, &mut note, false).unwrap();
        let saved_model: Note = jedi::from_val(val).unwrap();
        assert!(saved_model.id().is_some());
        assert_eq!(saved_model.space_id, space_id);
        assert!(saved_model.get_body().is_some());
        let id: String = saved_model.id().unwrap().clone();
        let notes: Vec<Note> = turtl.load_notes(&vec![id.clone()]).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, Some(String::from("my fav website LOL")));

        sync_model::delete_model::<Space>(&turtl, &space_id, false).unwrap();
        let profile_guard = lockr!(turtl.profile);
        let notes: Vec<Note> = turtl.load_notes(&vec![id.clone()]).unwrap();
        // we should have 0 spaces after removing the space
        assert_eq!(profile_guard.spaces.len(), 0);
        // and 0 notes because the space removal should remove all notes in that
        // space
        assert_eq!(notes.len(), 0);
    }

    #[test]
    fn syncs_outgoing() {
        let user_key = Key::new(crypto::from_base64(&String::from("jlz71VUIns1xM3Hq0fETZT98dxzhlqUxqb0VXYq1KtQ=")).unwrap());
        let mut user: User = jedi::parse(&String::from(r#"{"id":"51","username":"slippyslappy@turtlapp.com","storage":104857600}"#)).unwrap();
        let user_auth = String::from("000601000c9af06607bbb78b0cab4e01f2fda9887cf4fcdcb351527f9a1a134c7c89513241f8fc0d5d71341b46e792242dbce7d43f80e70d1c3c5c836e72b5bd861db35fed19cadf45d565fa95e7a72eb96ef464477271631e9ab375e74aa38fc752a159c768522f6fef1b4d8f1e29fdbcde59d52bfe574f3d600d6619c3609175f29331a353428359bcce95410d6271802275807c2fabd50d0189638afa7ce0a6");
        user.do_login(user_key, user_auth);

        let mut turtl = with_test(false);
        turtl.user = RwLock::new(user);
        {
            let user_guard = lockr!(turtl.user);
            let mut isengard = lockw!(turtl.user_id);
            *isengard = Some(user_guard.id().unwrap().clone());
        }

        let db = turtl.create_user_db().unwrap();
        turtl.db = Arc::new(Mutex::new(Some(db)));

        let mut space: Space = jedi::from_val(json!({
            "user_id":69,
            "title":"get a job"
        })).unwrap();
        // save our space to "disk"
        sync_model::save_model(SyncAction::Add, &turtl, &mut space, false).unwrap();

        // load our outgoing sync records and verify them
        let db_guard = lock!(turtl.db);
        let db = db_guard.as_ref().unwrap();
        let syncs: Vec<SyncRecord> = db.all("sync").unwrap();
        assert_eq!(syncs.len(), 2);
        assert_eq!(syncs[0].ty, SyncType::Keychain);
        assert_eq!(syncs[1].ty, SyncType::Space);
    }
}

