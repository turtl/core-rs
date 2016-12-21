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
use ::models::storable::Storable;

macro_rules! make_sync_incoming {
    ($n:ty) => {
        fn incoming(&self, db: &::std::sync::Arc<::storage::Storage>, sync_item: ::jedi::Value) -> ::error::TResult<()> {
            let sync_item = self.transform(sync_item)?;
            debug!("sync::incoming() -- {} / data: {}", self.model_type(), sync_item);
            let model_data = ::jedi::get(&["data"], &sync_item)?;
            let model: $n = ::jedi::from_val(model_data)?;
            self.saver(db, &model)
        }
    };
}

#[macro_export]
macro_rules! make_basic_sync_model {
    ($n:ty) => {
        impl ::sync::sync_model::SyncModel for $n {
            make_sync_incoming!{ $n }
        }
    };

    ($n:ty, $( $extra:tt )*) => {
        impl ::sync::sync_model::SyncModel for $n {
            make_sync_incoming!{ $n }

            $( $extra )*
        }
    };
}

pub trait SyncModel: Storable {
    /// A default save functoin that takes a db/model and saves it.
    fn saver<T>(&self, db: &Arc<Storage>, model: &T) -> TResult<()>
        where T: Protected + Storable
    {
        db.save(model)
    }

    /// Transform this model's data (if required).
    fn transform(&self, sync_item: Value) -> TResult<Value> {
        Ok(sync_item)
    }

    /// Run an incoming sync item
    fn incoming(&self, db: &Arc<Storage>, sync_item: Value) -> TResult<()>;
}

