//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;

use ::jedi::{self, Value};

use ::error::{TResult, TFutureResult, TError};
use ::util::event::{self, Emitter};
use ::storage::{self, Storage};
use ::api::Api;
use ::models::user::User;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::{Messenger, Response};
use ::sync::{self, SyncConfig};

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
    pub db: Option<Arc<Storage>>,
    /// Sync system configuration (shared state with the sync system).
    pub sync_config: Arc<RwLock<SyncConfig>>,
    /// Our external API object. Note that most things API-related go through
    /// the Sync system, but there are a handful of operations that Sync doesn't
    /// handle that need API access (Personas (soon to be deprecated) and
    /// invites come to mind). Use sparingly.
    pub api: Arc<Api>,
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<Turtl>;

impl Turtl {
    /// Create a new Turtl app
    fn new(tx_main: Pipeline, api: Arc<Api>, kv: Arc<Storage>) -> TResult<Turtl> {
        // TODO: match num processors - 1
        let num_workers = 3;

        // make sure we have a client id
        try!(storage::setup_client_id(kv.clone()));

        let mut turtl = Turtl {
            tx_main: tx_main.clone(),
            events: event::EventEmitter::new(),
            user: RwLock::new(User::new()),
            api: api,
            msg: Messenger::new(),
            work: Thredder::new("work", tx_main.clone(), num_workers),
            async: Thredder::new("async", tx_main.clone(), 2),
            kv: kv,
            db: None,
            sync_config: Arc::new(RwLock::new(SyncConfig::new())),
        };

        {
            let user_guard = turtl.user.read().unwrap();
            user_guard.bind("login", |_| {
                // TODO: init turtl.db w/ dumpy schema:
                //   let dumpy_schema = try!(config::get::<Value>(&["schema"]));
                // TODO: load profile into db
                // TODO: start sync system
            }, "turtl:user:login");

            user_guard.bind("logout", |_| {
            }, "turtl:user:logout");
        }

        Ok(turtl)
    }

    /// A handy wrapper for creating a wrapped Turtl object (TurtlWrap),
    /// shareable across threads.
    pub fn new_wrap(tx_main: Pipeline, api: Arc<Api>, kv: Arc<Storage>) -> TResult<TurtlWrap> {
        let turtl = Arc::new(try!(Turtl::new(tx_main, api, kv)));
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
        let msg = try!(jedi::stringify(&res));
        self.remote_send(Some(mid.clone()), msg)
    }

    /// Send an error response to a remote request
    pub fn msg_error(&self, mid: &String, err: &TError) -> TResult<()> {
        let res = Response {
            e: 1,
            d: Value::String(format!("{}", err)),
        };
        let msg = try!(jedi::stringify(&res));
        self.remote_send(Some(mid.clone()), msg)
    }

    /// Given that our API is synchronous but we need to not block th main
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
    pub fn start_sync(&mut self) -> TResult<()> {
        let db = match self.db {
            Some(ref x) => x.clone(),
            None => return Err(TError::MissingData(String::from("turtl.start_sync() -- missing `db` object"))),
        };
        let (_, _, sync_shutdown) = sync::start(self.tx_main.clone(), self.sync_config.clone(), db.clone());
        let shutdown_clone1 = Arc::new(sync_shutdown);
        let shutdown_clone2 = shutdown_clone1.clone();
        let user_guard = self.user.read().unwrap();
        user_guard.bind_once("logout", move |_| {
            shutdown_clone1();
        }, "turtl:user:logout:sync");
        self.events.bind_once("app:shutdown", move |_| {
            shutdown_clone2();
        }, "turtl:app:shutdown:sync");
        Ok(())
    }

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) {
    }
}

// Probably don't need this since `shutdown` just wipes our internal state which
// would happen anyway it Turtl is dropped, but whatever.
impl Drop for Turtl {
    fn drop(&mut self) {
        self.shutdown();
    }
}

