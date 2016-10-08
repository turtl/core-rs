//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;

use ::jedi::Value;

use ::error::TResult;
use ::util::event;
use ::storage::{self, Storage};
use ::api::Api;
use ::models::user::User;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::Messenger;

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User,
    pub api: Api,
    pub work: Thredder,
    pub msg: Messenger,
    pub kv: Option<Storage>,
    pub db: Option<Storage>,
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
    pub fn remote_send(&self, msg: String) -> TResult<()> {
        self.msg.send(msg)
    }

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) {
        self.kv = None;
        self.db = None;
        match self.msg.send_rev(String::from("turtl:internal:msg:shutdown")) {
            Ok(_) => (),
            Err(e) => {
                error!("turtl::shutdown() -- error shutting down messaging thread: {}", e)
            }
        }
    }
}

impl Drop for Turtl {
    fn drop(&mut self) {
        self.shutdown();
    }
}

