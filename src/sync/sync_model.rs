//! The `SyncModel` defines a trait that handles both incoming and outgoing sync
//! data. For instance, if we save a Note, the sync system will take the
//! encrypted note's data and run it through the NoteSync (which implements
//! SyncModel) before passing it off the the API. Conversely, if we grab changed
//! data from the API and it's a note, we pass it through the NoteSync object
//! which handles saving to the local disk.

use ::error::{TError, TResult};
use ::storage::Storage;
use ::sync::SyncRecord;
use ::models::protected::{Protected, Keyfinder};
use ::models::storable::Storable;
use ::jedi::Value;
use ::turtl::Turtl;
use ::models::model::Model;

macro_rules! make_sync_incoming {
    ($n:ty) => {
        fn incoming(&self, db: &::storage::Storage, sync_item: ::sync::SyncRecord) -> ::error::TResult<()> {
            if sync_item.action == "delete" {
                let mut model: $n = Default::default();
                model.id = Some(sync_item.item_id);
                model.db_delete(db)
            } else {
                if sync_item.data.is_none() {
                    return Err(::error::TError::MissingData(format!("missing `data` field in sync_item {} ({})", sync_item.id, self.model_type())));
                }
                let mut sync_item = self.transform(sync_item)?;
                let mut data = ::jedi::Value::Null;
                // swap the `data` out from under the SyncRecord so we don't
                // have to clone it
                ::std::mem::swap(sync_item.data.as_mut().unwrap(), &mut data);
                debug!("sync::incoming() -- {} / data: {:?}", self.model_type(), ::jedi::stringify(&data)?);
                let model: $n = ::jedi::from_val(data)?;
                model.db_save(db)
            }
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

pub trait SyncModel: Protected + Storable + Keyfinder + Sync + Send + 'static {
    /// Run an incoming sync item
    fn incoming(&self, db: &Storage, sync_item: SyncRecord) -> TResult<()>;

    /// A default save function that takes a db/model and saves it.
    fn db_save(&self, db: &Storage) -> TResult<()> {
        db.save(self)
    }

    /// A default delete function that takes a db/model and deletes it.
    fn db_delete(&self, db: &Storage) -> TResult<()> {
        db.delete(self)
    }

    /// Transform this model's data from an incoming sync (if required).
    fn transform(&self, sync_item: SyncRecord) -> TResult<SyncRecord> {
        Ok(sync_item)
    }

    /// Return a mutable reference to this model. Useful in cases where the
    /// model is wrapped in a container (RwLock, et al) and you need a ref to
    /// it.
    fn as_mut<'a>(&'a mut self) -> &'a mut Self {
        self
    }
}

pub trait MemorySaver: Protected {
    /// Save a model to Turtl's memory on save
    fn save_to_mem(self, _turtl: &Turtl) -> TResult<()> {
        Ok(())
    }

    /// Remove a model from Turtl's memory on delete
    fn remove_from_mem(&self, _turtl: &Turtl) -> TResult<()> {
        Ok(())
    }
}

/// Prepare a model for saving to db
fn prepare_for_sync<T>(turtl: &Turtl, model: &mut T) -> TResult<()>
    where T: Protected + Storable + Keyfinder + SyncModel + MemorySaver
{
    {
        let db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("sync_model::save_model() -- {}: turtl is missing `db` object", model.model_type()))),
        };

        if model.is_new() {
            model.generate_id()?;
            model.generate_key()?;
        } else {
            let got_model = db.get::<T>(model.table(), model.id().unwrap())?;
            match got_model {
                Some(db_model) => {
                    let model_data: Value = model.data()?;
                    model.merge_fields(&db_model.data_for_storage()?)?;
                    model.merge_fields(&model_data)?;
                },
                None => {},
            }
        }
    }

    turtl.find_model_key(model)?;
    let keyrefs = model.get_keyrefs(&turtl)?;
    model.generate_subkeys(&keyrefs)?;

    if model.add_to_keychain() {
        let user_id: String = {
            let user_guard = turtl.user.read().unwrap();
            (*user_guard).id().unwrap().clone()
        };
        let mut profile_guard = turtl.profile.write().unwrap();
        (*profile_guard).keychain.upsert_key(&user_id, model.id().as_ref().unwrap(), model.key().unwrap(), &String::from(model.model_type()), Some(turtl))?;
    }

    Ok(())
}

/// Called after a model is serialized and we want to save it/persist it
fn post_serialize<T>(turtl: &Turtl, model: T) -> TResult<Value>
    where T: Protected + Storable + Keyfinder + SyncModel + MemorySaver
{
    {
        let db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("sync_model::save_model() -- {}: turtl is missing `db` object", model.model_type()))),
        };
        model.db_save(db)?;
        // TODO: save to sync_outgoing
    }

    let model_data = model.data()?;
    model.save_to_mem(turtl)?;
    Ok(model_data)
}

/// Serialize this model and save it to the local db
///
/// TODO: is there a way around all the horrible cloning?
pub fn save_model<T>(turtl: &Turtl, model: &mut T) -> TResult<Value>
    where T: Protected + Storable + Keyfinder + SyncModel + MemorySaver + Sync + Send
{
    prepare_for_sync(turtl, model)?;
    let mut model2: T = model.clone()?;
    let serialized: Value = turtl.work.run(move || Protected::serialize(&mut model2))?;
    model.merge_fields(&serialized)?;
    post_serialize(turtl, model.clone()?)
}

/// Remove a model from memory/storage
pub fn delete_model<T>(turtl: &Turtl, id: &String) -> TResult<()>
    where T: Protected + Storable + SyncModel + MemorySaver
{
    let mut model: T = Default::default();
    model.set_id(id.clone());

    {
        let db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_ref() {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("sync_model::delete_model() -- {}: turtl is missing `db` object", model.model_type()))),
        };
        model.db_delete(db)?;
    }
    model.remove_from_mem(turtl)
}

