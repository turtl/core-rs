//! The `SyncModel` defines a trait that handles both incoming and outgoing sync
//! data. For instance, if we save a Note, the sync system will take the
//! encrypted note's data and run it through the NoteSync (which implements
//! SyncModel) before passing it off the the API. Conversely, if we grab changed
//! data from the API and it's a note, we pass it through the NoteSync object
//! which handles saving to the local disk.

use ::std::sync::Arc;

use ::jedi::Value;

use ::error::TResult;
use ::storage::Storage;
use ::models::protected::Protected;

#[macro_export]
macro_rules! make_basic_sync_model {
    ($n:ty) => {
        impl ::sync::sync_model::SyncModel for $n {
            fn incoming(&self, db: &::std::sync::Arc<::storage::Storage>, data: ::jedi::Value) -> ::error::TResult<()> {
                let model: $n = try!(::jedi::from_val(data));
                self.saver(db, &model)
            }
        }
    }
}

pub trait SyncModel {
    /// A default save functoin that takes a db/model and saves it.
    fn saver<T>(&self, db: &Arc<Storage>, model: &T) -> TResult<()>
        where T: Protected
    {
        db.save(model)
    }

    /// Run an incoming sync item
    fn incoming(&self, db: &Arc<Storage>, data: Value) -> TResult<()>;
}

