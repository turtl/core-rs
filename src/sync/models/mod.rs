//! The `SyncModel` defines a trait that handles both incoming and outgoing sync
//! data. For instance, if we save a Note, the sync system will take the
//! encrypted note's data and run it through the NoteSync (which implements
//! SyncModel) before passing it off the the API. Conversely, if we grab changed
//! data from the API and it's a note, we pass it through the NoteSync object
//! which handles saving to the local disk.

use ::jedi::Value;

use ::error::TResult;

pub mod user;
pub mod keychain;
pub mod persona;
pub mod board;
pub mod note;
pub mod file;
pub mod invite;

pub trait SyncModel {
    /// Get the table we operate on when saving data
    fn get_table(&self) -> &'static str;

    /// Run an incoming sync item
    fn incoming(&self, data: &Value) -> TResult<()> {
        Ok(())
    }

    /// Run an outgoing sync item
    fn outgoing(&self, data: &Value) -> TResult<()> {
        Ok(())
    }
}

