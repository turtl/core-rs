//! The sync system is responsible for syncing the data we have stored in our
//! local db with the Turtl server we're talking to.
//!
//! The idea is that all changes to data happen locally, and in fact Turtl is
//! capable of working entirely offline, but once we connect to a server we sync
//! all the outstanding changes to/from the server such that every piece of data
//! is synced.
//!
//! That's the goal, anyway.

#[cfg(test)]
mod tests {
    #[test]
    fn make_sure_everything_works_lol() {
    }
}

