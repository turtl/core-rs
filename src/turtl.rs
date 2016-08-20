//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::util::event;
use ::api::Api;
use ::models::user::User;
use ::util::thredder::Pipeline;
use ::messaging::{Messenger, MsgSender};
use ::error::{TError, TResult};

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User,
    pub api: Api,
    pub msg: Option<MsgSender>,
}

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx_main: Pipeline, tx_msg: MsgSender) -> Turtl {
        Turtl {
            events: event::EventEmitter::new(),
            user: User::blank(),
            api: Api::new(String::new(), tx_main),
            msg: Some(tx_msg),
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

    pub fn shutdown(self) {
        match self.with_remote_sender(|messenger| { messenger.shutdown(); }) {
            Err(e) => error!("dispatch: error shutting down messenger thread: {}", e),
            _ => (),
        }
    }
}

