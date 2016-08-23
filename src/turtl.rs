//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::std::sync::{Arc, RwLock};

use ::util::event;
use ::api::Api;
use ::models::user::User;
use ::util::thredder::{Thredder, Pipeline};
use ::messaging::{Messenger, MsgSender};
use ::error::{TError, TResult};

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User,
    pub api: Api,
    pub work: Thredder,
    pub msg: Option<MsgSender>,
}

/// A handy type alias for passing Turtl around
pub type TurtlWrap = Arc<RwLock<Turtl>>;

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx_main: Pipeline, tx_msg: MsgSender) -> Turtl {
        // TODO: match num processors - 1
        let num_workers = 3;
        Turtl {
            events: event::EventEmitter::new(),
            user: User::blank(),
            api: Api::new(String::new(), tx_main.clone()),
            msg: Some(tx_msg),
            work: Thredder::new("work", tx_main.clone(), num_workers),
        }
    }

    pub fn with_remote_sender<F>(&self, cb: F) -> TResult<()>
        where F: FnOnce(&mut Messenger) + Send + 'static
    {
        let sender = &self.msg;
        match *sender {
            Some(ref x) => {
                x.send(Box::new(cb)).map_err(|e| toterr!(e))
            },
            None => Err(TError::MissingField(format!("turtl: missing `msg`"))),
        }
    }

    pub fn remote_send(&self, msg: String) -> TResult<()> {
        self.with_remote_sender(move |messenger| {
            match messenger.send(msg) {
                Ok(..) => (),
                Err(e) => error!("turtl: remote_send: {:?}", e),
            }
        })
    }

    pub fn shutdown(&mut self) {
        match self.with_remote_sender(|messenger| { messenger.shutdown(); }) {
            Err(e) => error!("dispatch: error shutting down messenger thread: {}", e),
            _ => (),
        }
    }
}

