//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;

use ::jedi::Value;

use ::error::{TError, TResult};
use ::util::event;
use ::storage::{self, Storage};
use ::api::Api;
use ::models::user::User;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::{Messenger, MsgSender};

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User,
    pub api: Api,
    pub work: Thredder,
    pub msg: Option<MsgSender>,
    pub kv: Option<Storage>,
    pub db: Option<Storage>,
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<RwLock<Turtl>>;

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx_main: Pipeline, tx_msg: MsgSender, data_folder: &String, dumpy_schema: Value) -> TResult<Turtl> {
        // TODO: match num processors - 1
        let num_workers = 3;
        let kv = try!(Storage::new(&format!("{}/kv.sqlite", data_folder), dumpy_schema));

        // make sure we have a client id
        try!(storage::setup_client_id(&kv));

        let turtl = Turtl {
            events: event::EventEmitter::new(),
            user: User::new(),
            api: Api::new(String::new(), tx_main.clone()),
            msg: Some(tx_msg),
            work: Thredder::new("work", tx_main.clone(), num_workers),
            kv: Some(kv),
            db: None,
        };
        Ok(turtl)
    }

    /// A handy wrapper for creating a wrapped Turtl object (TurtlWrap),
    /// shareable across threads.
    pub fn new_wrap(tx_main: Pipeline, tx_msg: MsgSender, data_folder: &String, dumpy_schema: Value) -> TResult<TurtlWrap> {
        let turtl = try!(Turtl::new(tx_main, tx_msg, data_folder, dumpy_schema));
        Ok(Arc::new(RwLock::new(turtl)))
    }

    /// Wrapper to handle making sending messages a bit nicer. you probably want
    /// `Turtl.remote_send` instead.
    pub fn with_remote_sender<F>(&self, cb: F) -> TResult<()>
        where F: FnOnce(&mut Messenger) + Send + 'static
    {
        let sender = &self.msg;
        match *sender {
            Some(ref x) => {
                x.push(Box::new(cb));
                Ok(())
            },
            None => Err(TError::MissingField(format!("turtl::with_remote_sender() -- missing `turtl.msg`"))),
        }
    }

    /// Send a message to (presumably) our UI.
    pub fn remote_send(&self, msg: String) -> TResult<()> {
        self.with_remote_sender(move |messenger| {
            match messenger.send(msg) {
                Ok(..) => (),
                Err(e) => error!("turtl::remote_send() -- {:?}", e),
            }
        })
    }

    /// Shut down this Turtl instance and all the state/threads it manages
    pub fn shutdown(&mut self) {
        self.kv = None;
        self.db = None;
        match self.with_remote_sender(|messenger| messenger.shutdown()) {
            Err(e) => error!("turtl::shutdown() -- error shutting down messenger thread: {:?}", e),
            _ => (),
        }
    }
}

impl Drop for Turtl {
    fn drop(&mut self) {
        self.shutdown();
    }
}

