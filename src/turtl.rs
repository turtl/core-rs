//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};
use ::std::ops::Drop;

use ::error::{TError, TResult};
use ::util::event;
use ::storage::Storage;
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
    pub db: Storage,
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<RwLock<Turtl>>;

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx_main: Pipeline, tx_msg: MsgSender, db_location: &String) -> TResult<Turtl> {
        // TODO: match num processors - 1
        let num_workers = 3;
        let turtl = Turtl {
            events: event::EventEmitter::new(),
            user: User::new(),
            //storage: Storage::new(db_location),
            api: Api::new(String::new(), tx_main.clone()),
            msg: Some(tx_msg),
            work: Thredder::new("work", tx_main.clone(), num_workers),
            db: try!(Storage::new(tx_main.clone(), db_location)),
        };
        Ok(turtl)
    }

    /// A handy wrapper for creating a wrapped Turtl object (TurtlWrap),
    /// shareable across threads.
    pub fn new_wrap(tx_main: Pipeline, tx_msg: MsgSender, db_location: &String) -> TResult<TurtlWrap> {
        let turtl = try!(Turtl::new(tx_main, tx_msg, db_location));
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
        self.db.stop();
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

