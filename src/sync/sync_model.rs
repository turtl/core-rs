//! The `SyncModel` defines a trait that handles both incoming and outgoing sync
//! data. For instance, if we save a Note, the sync system will take the
//! encrypted note's data and run it through the NoteSync (which implements
//! SyncModel) before passing it off the the API. Conversely, if we grab changed
//! data from the API and it's a note, we pass it through the NoteSync object
//! which handles saving to the local disk.

use ::error::{TError, TResult};
use ::storage::Storage;
use ::models::protected::{Protected, Keyfinder};
use ::models::storable::Storable;
use ::jedi::Value;
use ::turtl::Turtl;
use ::models::sync_record::{SyncAction, SyncRecord};

macro_rules! make_sync_incoming {
    ($n:ty) => {
        fn incoming(&self, db: &mut ::storage::Storage, sync_item: ::models::sync_record::SyncRecord) -> ::error::TResult<()> {
            match sync_item.action {
                ::models::sync_record::SyncAction::Delete => {
                    let mut model: $n = Default::default();
                    model.id = Some(sync_item.item_id);
                    model.db_delete(db)
                }
                _ => {
                    if sync_item.data.is_none() {
                        return Err(::error::TError::MissingData(format!("missing `data` field in sync_item {} ({})", sync_item.id.unwrap_or(String::from("<no id>")), self.model_type())));
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
        }

        fn outgoing(&self, action: ::models::sync_record::SyncAction, user_id: &String, db: &mut ::storage::Storage, skip_remote_sync: bool) -> ::error::TResult<()> {
            match action {
                ::models::sync_record::SyncAction::Delete => {
                    self.db_delete(db)?;
                }
                _ => {
                    self.db_save(db)?;
                }
            }
            if skip_remote_sync { return Ok(()); }

            let mut sync_record = ::models::sync_record::SyncRecord::default();
            sync_record.generate_id()?;
            sync_record.action = action.clone();
            sync_record.user_id = user_id.clone();
            sync_record.ty = String::from(self.model_type());
            sync_record.item_id = match self.id() {
                Some(id) => id.clone(),
                None => return Err(::error::TError::MissingField(format!("SyncModel::outgoing() -- model ({}) is missing its id", self.model_type()))),
            };
            match action {
                ::models::sync_record::SyncAction::Delete => {
                    sync_record.data = Some(json!({
                        "id": self.id().unwrap().clone(),
                    }));
                }
                _ => {
                    sync_record.data = Some(self.data_for_storage()?);
                }
            }
            sync_record.db_save(db)
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
    /// Allows a model to handle an incoming sync item for its type.
    fn incoming(&self, db: &mut Storage, sync_item: SyncRecord) -> TResult<()>;

    /// Allows a model to save itself to the outgoing sync database (or perform
    /// any custom needed actual in addition/instead).
    fn outgoing(&self, action: SyncAction, user_id: &String, db: &mut Storage, skip_remote_sync: bool) -> ::error::TResult<()>;

    /// A default save function that takes a db/model and saves it.
    fn db_save(&self, db: &mut Storage) -> TResult<()> {
        db.save(self)
    }

    /// A default delete function that takes a db/model and deletes it.
    fn db_delete(&self, db: &mut Storage) -> TResult<()> {
        db.delete(self)
    }

    /// Transform this model's data from an incoming sync (if required).
    fn transform(&self, sync_item: SyncRecord) -> TResult<SyncRecord> {
        Ok(sync_item)
    }
}

pub trait MemorySaver: Protected {
    /// Save a model to Turtl's memory on save
    fn save_to_mem(self, _turtl: &Turtl) -> TResult<()> {
        Ok(())
    }

    /// Remove a model from Turtl's memory on delete
    fn delete_from_mem(&self, _turtl: &Turtl) -> TResult<()> {
        Ok(())
    }
}

/// Serialize this model and save it to the local db
///
pub fn save_model<T>(action: SyncAction, turtl: &Turtl, model: &mut T, skip_remote_sync: bool) -> TResult<Value>
    where T: Protected + Storable + Keyfinder + SyncModel + MemorySaver + Sync + Send
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
                None => (),
            }
        }
    }

    turtl.find_model_key(model)?;
    let keyrefs = model.get_keyrefs(&turtl)?;
    model.generate_subkeys(&keyrefs)?;

    if model.add_to_keychain() {
        let mut profile_guard = turtl.profile.write().unwrap();
        (*profile_guard).keychain.upsert_key_save(turtl, model.id().as_ref().unwrap(), model.key().unwrap(), &String::from(model.model_type()), skip_remote_sync)?;
    }

    // TODO: is there a way around all the horrible cloning?
    let mut model2: T = model.clone()?;
    let serialized: Value = turtl.work.run(move || Protected::serialize(&mut model2))?;
    model.merge_fields(&serialized)?;

    {
        let user_id = {
            let isengard = turtl.user_id.read().unwrap();
            match *isengard {
                Some(ref id) => id.clone(),
                None => return Err(TError::MissingField(String::from("sync_model::save_model() -- turtl.user_id has failed us..."))),
            }
        };
        let mut db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("sync_model::save_model() -- {}: turtl is missing `db` object", model.model_type()))),
        };
        model.outgoing(action, &user_id, db, skip_remote_sync)?;
    }

    let model_data = model.data()?;
    // TODO: is there a way around all the horrible cloning?
    model.clone()?.save_to_mem(turtl)?;
    Ok(model_data)
}

/// Remove a model from memory/storage
pub fn delete_model<T>(turtl: &Turtl, id: &String, skip_remote_sync: bool) -> TResult<()>
    where T: Protected + Storable + SyncModel + MemorySaver
{
    let mut model: T = Default::default();
    model.set_id(id.clone());

    {
        let user_id = {
            let isengard = turtl.user_id.read().unwrap();
            match *isengard {
                Some(ref id) => id.clone(),
                None => return Err(TError::MissingField(String::from("sync_model::delete_model() -- turtl.user_id has failed us..."))),
            }
        };
        let mut db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(format!("sync_model::delete_model() -- {}: turtl is missing `db` object", model.model_type()))),
        };
        model.outgoing(SyncAction::Delete, &user_id, db, skip_remote_sync)?;
    }
    model.delete_from_mem(turtl)
}

