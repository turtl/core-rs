//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::sqlite::{self, Connection};

use ::error::TResult;

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    conn: Connection
}

impl Storage {
    /// Make a Storage lol
    pub fn new(location: &String) -> TResult<Storage> {
        Ok(Storage {
            conn: try!(sqlite::open(&location[..])),
        })
    }
}

