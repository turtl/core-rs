//! This module holds our search system.
//!
//! It implements the full-text capabilities of our Clouseau crate, as well as
//! adding some Turtl-specific indexing to the Clouseau sqlite connection.

use ::clouseau::Clouseau;

/// Holds the state for our search
pub struct Search {
    /// Our full-text index
    pub ft: Clouseau
}

unsafe impl Send for Search {}
unsafe impl Sync for Search {}

