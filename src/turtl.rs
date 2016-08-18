//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::util::event;
use ::api::Api;
use ::models::user::User;
use ::util::thredder::Pipeline;

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User,
    pub api: Api
}

impl Turtl {
    /// Create a new Turtl app
    pub fn new(tx: Pipeline) -> Turtl {
        Turtl {
            events: event::EventEmitter::new(),
            user: User::blank(),
            api: Api::new(String::new(), tx),
        }
    }
}

