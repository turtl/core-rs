//! The sync system is responsible for syncing the data we have stored in our
//! local db with the Turtl server we're talking to.
//!
//! The idea is that all changes to data happen locally, and in fact Turtl is
//! capable of working entirely offline, but once we connect to a server we sync
//! all the outstanding changes to/from the server such that every piece of data
//! is synced.
//!
//! That's the goal, anyway.
//!
//! Keep in mind that ALL data that goes through the sync system needs to be
//! either a) public or b) encrypted. The sync system shall not ever touch
//! plaintext private data. Keeping this in mind, the actual API object should
//! be a separate instance from the one used in the Turtl object, and the Sync
//! object should always be in a separate thread from the main Turtl object.
//!
//! NOTE: This is going to be an almost verbatim copy of the turtl/js
//! models/_sync.js file, ported to Rust. Save for one or two bugs, the Turtl
//! sync system works fantastically so is a good starting point.
//! NOTE: The conversion won't be 100% because the javascript sync system is
//! async, whereas this will be sync. However, this should actually simplify the
//! design.
//! NOTE: AL: might need to have two threads? Outgoing and incoming?

mod sync_model;

use ::std::collections::HashMap;

use ::sync::sync_model::SyncModel;

pub struct Sync {
    /// A collection of `sync_id`s (that the API hands us after each write /
    /// outgoing sync) which we should ignore when they come back. For isntance,
    /// we might send a note, and get sync_id 1234 back. The next time we poll
    /// for changes, we *will* get that note back...do we "add" it again to our
    /// data?
    /// TODO: make the sync system resilient to these kinds of shenanigans so we
    /// don't have to track this at all. For instance, just track updates and
    /// deletes. `update` should add if it doesnt exist, otherwise update.
    ignore_on_next: HashMap<String, bool>,
    /// Are we enabled?
    enabled: bool,

    /// Tracks whether or not we're connected.
    /// TODO: should just signal the main thread
    connected: bool,
    /// Tracks if we have an outgoing poll running already, and if so, don't run
    /// another one.
    /// TODO: remove, async
    _polling: bool,
    /// Tracks if we're pushing data to the API
    /// TODO: remove, async
    _outgoing_sync_running: bool,

    /// For each type we get back from an outgoing poll, defines a collection
    /// that is able to handle that incoming item (for instance a "note" coming
    /// from the API might get handled by the NoteCollection).
    /// TODO: rename to `trackers` (duhh, they're local) or something better
    local_trackers: HashMap<String, Box<SyncModel>>,

    /// Tracks the current outgoing poll
    poll_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_sure_everything_works_lol() {
    }
}

