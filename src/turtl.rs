//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;

use ::jedi::{self, Value};
use ::serde::ser::{Serialize, Serializer};

use ::error::{TError, TResult};
use ::util::event;
use ::storage::{self, Storage};
use ::api::Api;
use ::models::user::User;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::Messenger;

/// Defines a container for our app's state. Note that most operations the user
/// has access to via messaging get this object passed to them.
pub struct Turtl {
    /// This is our app-wide event bus.
    pub events: event::EventEmitter,
    /// Holds our current user (Turtl only allows one logged-in user at once)
    pub user: User,
    /// Our external API object. Note that most things API-related go through
    /// the Sync system, but there are a handful of operations that Sync doesn't
    /// handle that need API access (Personas (soon to be deprecated) and
    /// invites come to mind). Use sparingly.
    pub api: Api,
    /// Need to do some CPU-intensive work and have a Future finished when it's
    /// done? Send it here! Great for decrypting models.
    pub work: Thredder,
    /// Allows us to send messages to our UI
    pub msg: Messenger,
    /// A storage system dedicated to key-value data. This *must* be initialized
    /// before our main local db because our local db is baed off the currently
    /// logged-in user, and we need persistant key-value storage even when
    /// logged out.
    pub kv: Option<Storage>,
    /// Our main database, initialized after a successful login. This db is
    /// named via a function of the user ID and the server we're talking to,
    /// meaning we can have multiple databases that store different things for
    /// different people depending on server/user.
    pub db: Option<Storage>,
}

/// Defines a container for sending responses to the client. We could use a hash
/// table, but then the elements might serialize out of order. This allows us to
/// force our "error" key (`e`) first, and put "data" (`d`) second.
///
/// Note that this is more or less a Turtl-enforced RPC system. Each "call" we
/// run has a response of either error (`e = 1`) or success (`e = 0`) and
/// any supporting data (the error that occurred, or the data we requested).
struct Response {
    /// `e > 0` means "error!!!1", `e == 0` means "great success!!"
    e: i64,
    /// Any data we want to pass back to the UI
    d: Value,
}

// Make `Response` Serde serializable
impl Serialize for Response {
    fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
        where S: Serializer
    {
        let mut state = try!(serializer.serialize_struct("res", 2));
        try!(serializer.serialize_struct_elt(&mut state, "e", &self.e));
        try!(serializer.serialize_struct_elt(&mut state, "d", &self.d));
        serializer.serialize_struct_end(state)
    }
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<RwLock<Turtl>>;

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx_main: Pipeline, msg_channel: String, data_folder: &String, dumpy_schema: Value) -> TResult<Turtl> {
        // TODO: match num processors - 1
        let num_workers = 3;
        let kv = try!(Storage::new(&format!("{}/kv.sqlite", data_folder), dumpy_schema));

        // make sure we have a client id
        try!(storage::setup_client_id(&kv));

        let turtl = Turtl {
            events: event::EventEmitter::new(),
            user: User::new(),
            api: Api::new(String::new(), tx_main.clone()),
            msg: Messenger::new(msg_channel),
            work: Thredder::new("work", tx_main.clone(), num_workers),
            kv: Some(kv),
            db: None,
        };
        Ok(turtl)
    }

    /// A handy wrapper for creating a wrapped Turtl object (TurtlWrap),
    /// shareable across threads.
    pub fn new_wrap(tx_main: Pipeline, msg_channel: String, data_folder: &String, dumpy_schema: Value) -> TResult<TurtlWrap> {
        let turtl = try!(Turtl::new(tx_main, msg_channel, data_folder, dumpy_schema));
        Ok(Arc::new(RwLock::new(turtl)))
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

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) {
        self.kv = None;
        self.db = None;
    }
}

// Probably don't need this since `shutdown` just wipes our internal state which
// would happen anyway it Turtl is dropped, but whatever.
impl Drop for Turtl {
    fn drop(&mut self) {
        self.shutdown();
    }
}

