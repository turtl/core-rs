//! The Turtl module is the container for the state of the app. It provides
//! functions/interfaces for updating or retrieving stateful info about the app.

use ::util::event;
use ::models::user::User;
use std::sync::RwLock;

/// Defines a container for our app's state
struct Turtl<'event> {
    pub events: event::EventEmitter<'event>,
    pub user: User<'event>
}

/*
lazy_static! {
    static ref TURTL: RwLock<Turtl> = RwLock::new(Turtl {
        events: event::EventEmitter::new(),
        user: User::blank(),
    });
}
*/

