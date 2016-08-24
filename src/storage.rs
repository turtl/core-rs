//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::sqlite3::access;
use ::sqlite3::core::DatabaseConnection;

use ::error::{TResult, TError};

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    conn: DatabaseConnection
}

impl Storage {
    /// Make a Storage lol
    pub fn new(location: &String) -> TResult<Storage> {
        Ok(Storage {
            conn: try_t!(access::open(&location[..], None)),
        })
    }
}

