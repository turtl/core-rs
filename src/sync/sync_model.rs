//! The `SyncModel` defines a trait that handles both incoming and outgoing sync
//! data. For instance, if we save a Note, the sync system will take the
//! encrypted note's data and run it through the NoteSync (which implements
//! SyncModel) before passing it off the the API. Conversely, if we grab changed
//! data from the API and it's a note, we pass it through the NoteSync object
//! which handles saving to the local disk.

use ::error::{TError, TResult};
use ::storage::Storage;
use ::models::model::Model;
use ::models::protected::{Protected, Keyfinder};
use ::models::sync_record::{SyncType, SyncAction, SyncRecord};
use ::models::storable::Storable;
use ::jedi::{self, Value};
use ::turtl::Turtl;
use ::std::mem;

pub trait SyncModel: Protected + Storable + Keyfinder + Sync + Send + 'static {
    /// Allows a model to handle an incoming sync item for its type.
    fn incoming(&self, db: &mut Storage, sync_item: &mut SyncRecord) -> TResult<()> {
        if self.skip_incoming_sync(&sync_item)? {
            return Ok(());
        }
        match sync_item.action {
            SyncAction::Delete => {
                let mut model: Self = Default::default();
                model.set_id(sync_item.item_id.clone());
                model.db_delete(db, Some(sync_item as &SyncRecord))
            }
            _ => {
                if sync_item.data.is_none() {
                    let sync_id = sync_item.id().map(|x| x.as_str()).unwrap_or("<no id>");
                    return TErr!(TError::MissingField(format!("SyncItem.data ({} / {})", sync_id, self.model_type())));
                }
                self.transform(sync_item)?;
                let mut data = Value::Null;
                // swap the `data` out from under the SyncRecord so we don't
                // have to clone it
                mem::swap(sync_item.data.as_mut().unwrap(), &mut data);
                debug!("sync::incoming() -- {} / data: {:?}", self.model_type(), jedi::stringify(&data)?);
                let model: Self = jedi::from_val(data)?;
                model.db_save(db, Some(sync_item as &SyncRecord))?;
                // set the data back into the sync record so's we'll have it
                // handy when we run our trusty sync handler
                sync_item.data = Some(model.data_for_storage()?);
                Ok(())
            }
        }
    }

    /// Allows a model to save itself to the outgoing sync database (or perform
    /// any custom needed actual in addition/instead).
    fn outgoing(&self, action: SyncAction, user_id: &String, db: &mut Storage, skip_remote_sync: bool) -> TResult<()> {
        match action {
            SyncAction::Delete => {
                self.db_delete(db, None)?;
            }
            _ => {
                self.db_save(db, None)?;
            }
        }
        if skip_remote_sync { return Ok(()); }

        let mut sync_record = SyncRecord::default();
        sync_record.generate_id()?;
        sync_record.action = action;
        sync_record.user_id = user_id.clone();
        sync_record.ty = SyncType::from_string(self.model_type())?;
        sync_record.item_id = match self.id() {
            Some(id) => id.clone(),
            None => return TErr!(TError::MissingField(format!("{}.id", self.model_type()))),
        };
        match sync_record.action {
            SyncAction::Delete => {
                sync_record.data = Some(json!({
                    "id": self.id().unwrap().clone(),
                }));
            }
            _ => {
                sync_record.data = Some(self.data_for_storage()?);
            }
        }
        sync_record.db_save(db, None)
    }

    /// Gives us the option to skip an incoming sync. Some sync records are just
    /// indicators for something happening as opposed to data changes (for
    /// instance the "change-password" sync action).
    fn skip_incoming_sync(&self, _sync_item: &SyncRecord) -> TResult<bool> {
        Ok(false)
    }

    /// A default save function that takes a db/model and saves it.
    fn db_save(&self, db: &mut Storage, _sync_item: Option<&SyncRecord>) -> TResult<()> {
        db.save(self)
    }

    /// A default delete function that takes a db/model and deletes it.
    fn db_delete(&self, db: &mut Storage, _sync_item: Option<&SyncRecord>) -> TResult<()> {
        db.delete(self)
    }

    /// Transform this model's data from an incoming sync (if required).
    fn transform(&self, _sync_item: &mut SyncRecord) -> TResult<()> {
        Ok(())
    }
}

pub trait MemorySaver: Protected {
    /// Update in-mem state based on sync item
    fn mem_update(self, _turtl: &Turtl, _action: SyncAction) -> TResult<()> {
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
            None => return TErr!(TError::MissingField(format!("Turtl.db ({})", model.model_type()))),
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
        let user_id = turtl.user_id()?;
        let mut db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_mut() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(format!("Turtl.db ({})", model.model_type()))),
        };
        model.outgoing(action.clone(), &user_id, db, skip_remote_sync)?;
    }

    let model_data = model.data()?;
    // TODO: is there a way around all the horrible cloning?
    model.clone()?.mem_update(turtl, action)?;
    Ok(model_data)
}

/// Remove a model from memory/storage
pub fn delete_model<T>(turtl: &Turtl, id: &String, skip_remote_sync: bool) -> TResult<()>
    where T: Protected + Storable + SyncModel + MemorySaver
{
    let mut model: T = Default::default();
    model.set_id(id.clone());

    // if this model adds itself to the keychain on create, then it should be
    // removed from the keychain on delete.
    if model.add_to_keychain() {
        let mut profile_guard = turtl.profile.write().unwrap();
        (*profile_guard).keychain.remove_entry(model.id().as_ref().unwrap(), Some((turtl, skip_remote_sync)))?;
    }

    {
        let user_id = turtl.user_id()?;
        let mut db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_mut() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(format!("Turtl.db ({})", model.model_type()))),
        };
        model.outgoing(SyncAction::Delete, &user_id, db, skip_remote_sync)?;
    }
    model.mem_update(turtl, SyncAction::Delete)
}

