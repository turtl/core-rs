//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::util::event;
use ::models::user::User;

/// Defines a container for our app's state
pub struct Turtl {
    pub events: event::EventEmitter,
    pub user: User
}

impl Turtl {
    /// Create a new Turtl app
    pub fn new() -> Turtl {
        Turtl {
            events: event::EventEmitter::new(),
            user: User::blank(),
        }
    }
}

