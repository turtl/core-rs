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
use ::models::keychain;
use ::models::sync_record::{SyncType, SyncAction, SyncRecord};
use ::models::storable::Storable;
use ::models::user::User;
use ::models::space::Space;
use ::models::board::Board;
use ::models::note::Note;
use ::models::file::FileData;
use ::profile::Profile;
use ::lib_permissions::Permission;
use ::jedi::{self, Value};
use ::turtl::Turtl;
use ::std::mem;
use ::messaging;

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

                // if we're running an update and our object's data is missing,
                // don't bother. odds are the sync item directly after this is a
                // delete =]
                let has_missing: Option<bool> = jedi::get_opt(&["missing"], sync_item.data.as_ref().unwrap());
                if has_missing.is_some() {
                    return Ok(());
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
        sync_record.item_id = self.id_or_else()?;
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
    /// Update in-mem state based on sync item. Generally, models will overwrite
    /// this with custom code that updates whatever respective state is in the
    /// Turtl object.
    fn mem_update(self, _turtl: &Turtl, _action: SyncAction) -> TResult<()> {
        Ok(())
    }

    /// Our in-app entry point for calling mem_update(). Does some other nice
    /// things for us like alerting the UI that a model changed in-mem.
    fn run_mem_update(self, turtl: &Turtl, action: SyncAction) -> TResult<()> {
        let mut sync_item = SyncRecord::default();
        sync_item.action = action.clone();
        sync_item.user_id = String::from("0");
        sync_item.item_id = self.id_or_else()?;
        sync_item.ty = SyncType::from_string(self.model_type())?;
        sync_item.data = Some(self.data()?);
        self.mem_update(turtl, action)?;
        messaging::ui_event("sync:update", &sync_item)
    }
}

/// Serialize this model and save it to the local db
pub fn save_model<T>(action: SyncAction, turtl: &Turtl, model: &mut T, skip_remote_sync: bool) -> TResult<Value>
    where T: Protected + Storable + Keyfinder + SyncModel + MemorySaver + Sync + Send
{
    {
        let db_guard = turtl.db.write().unwrap();
        let db = match (*db_guard).as_ref() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(format!("Turtl.db ({})", model.model_type()))),
        };

        if action == SyncAction::Add {
            model.generate_id()?;
            model.generate_key()?;
        } else {
            let got_model = db.get::<T>(model.table(), model.id().unwrap())?;
            match got_model {
                Some(db_model) => {
                    let mut model_data: Value = model.data()?;
                    // users can't directly edit object ownership
                    jedi::remove(&["user_id"], &mut model_data)?;
                    model.merge_fields(&db_model.data_for_storage()?)?;
                    model.merge_fields(&model_data)?;
                    match db_model.get_keys() {
                        Some(keys) => {
                            model.set_keys(keys.clone());
                        }
                        None => {}
                    }
                },
                None => (),
            }
        }
    }

    turtl.find_model_key(model)?;
    let keyrefs = model.get_keyrefs(&turtl)?;
    model.generate_subkeys(&keyrefs)?;

    if model.add_to_keychain() {
        keychain::save_key(turtl, model.id().as_ref().unwrap(), model.key().unwrap(), &String::from(model.model_type()), skip_remote_sync)?;
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
    model.clone()?.run_mem_update(turtl, action.clone())?;
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
        keychain::remove_key(turtl, model.id().as_ref().unwrap(), skip_remote_sync)?;
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
    model.run_mem_update(turtl, SyncAction::Delete)?;
    Ok(())
}

/// Given a sync record, dispatch it into the sync system, calling the
/// appropriate functions and running any permissions checks.
pub fn dispatch(turtl: &Turtl, sync_record: SyncRecord) -> TResult<Value> {
    let SyncRecord {action, ty, data: modeldata_maybe, ..} = sync_record;
    let mut modeldata = match modeldata_maybe {
        Some(x) => x,
        None => return TErr!(TError::MissingField(String::from("sync_record.data"))),
    };

    match action.clone() {
        SyncAction::Add | SyncAction::Edit => {
            let val = match ty {
                SyncType::User => {
                    if action != SyncAction::Edit {
                        return TErr!(TError::BadValue(format!("cannot `add` item of type {:?}", ty)));
                    }
                    let mut model: User = jedi::from_val(modeldata)?;
                    save_model(action, turtl, &mut model, false)?
                }
                SyncType::Space => {
                    let mut model: Space = jedi::from_val(modeldata)?;
                    match &action {
                        &SyncAction::Edit => {
                            let fake_id = String::from("<no id>");
                            let space_id = model.id().unwrap_or(&fake_id);
                            Space::permission_check(turtl, space_id, &Permission::EditSpace)?;
                        }
                        &SyncAction::Add => {
                            model.user_id = turtl.user_id()?;
                        }
                        _ => {}
                    };
                    save_model(action, turtl, &mut model, false)?
                }
                SyncType::Board => {
                    let mut model: Board = jedi::from_val(modeldata)?;
                    let permission = match &action {
                        &SyncAction::Add => Permission::AddBoard,
                        &SyncAction::Edit => Permission::EditBoard,
                        _ => return TErr!(TError::BadValue(format!("couldn't find permission for {:?}/{:?}", ty, action))),
                    };
                    Space::permission_check(turtl, &model.space_id, &permission)?;
                    if action == SyncAction::Add {
                        model.user_id = turtl.user_id()?;
                    }
                    save_model(action, turtl, &mut model, false)?
                }
                SyncType::Note => {
                    let filemebbe: Option<FileData> = jedi::get_opt(&["file", "filedata"], &modeldata);
                    match jedi::remove(&["file", "filedata"], &mut modeldata) {
                        Ok(_) => {}
                        Err(_) => {}
                    }
                    let mut note: Note = jedi::from_val(modeldata)?;
                    let permission = match &action {
                        &SyncAction::Add => Permission::AddNote,
                        &SyncAction::Edit => Permission::EditNote,
                        _ => return TErr!(TError::BadValue(format!("couldn't find permission for {:?}/{:?}", ty, action))),
                    };
                    Space::permission_check(turtl, &note.space_id, &permission)?;
                    if action == SyncAction::Add {
                        note.user_id = turtl.user_id()?;
                    }
                    // always set to false. this is a public field that
                    // we let the server manage for us
                    note.has_file = false;
                    let note_data = save_model(action, turtl, &mut note, false)?;
                    match filemebbe {
                        Some(mut file) => {
                            file.save(turtl, &mut note)?;
                        }
                        None => {}
                    }
                    note_data
                }
                _ => {
                    return TErr!(TError::BadValue(format!("cannot direct sync an item of type {:?}", ty)));
                }
            };
            Ok(val)
        }
        SyncAction::Delete => {
            let id: String = jedi::get(&["id"], &modeldata)?;
            fn get_model<T>(turtl: &Turtl, id: &String) -> TResult<T>
                where T: Protected + Storable
            {
                let mut db_guard = turtl.db.write().unwrap();
                let db = match db_guard.as_mut() {
                    Some(x) => x,
                    None => return TErr!(TError::MissingField(format!("turtl is missing `db` object"))),
                };
                match db.get::<T>(T::tablename(), id)? {
                    Some(x) => Ok(x),
                    None => return TErr!(TError::NotFound(format!("that {} model wasn't found", T::tablename()))),
                }
            }
            match ty {
                SyncType::Space => {
                    Space::permission_check(turtl, &id, &Permission::DeleteSpace)?;
                    delete_model::<Space>(turtl, &id, false)?;
                }
                SyncType::Board => {
                    let model = get_model::<Board>(turtl, &id)?;
                    Space::permission_check(turtl, &model.space_id, &Permission::DeleteBoard)?;
                    delete_model::<Board>(turtl, &id, false)?;
                }
                SyncType::Note => {
                    let model = get_model::<Note>(turtl, &id)?;
                    Space::permission_check(turtl, &model.space_id, &Permission::DeleteNote)?;
                    delete_model::<Note>(turtl, &id, false)?;
                }
                SyncType::File => {
                    let model = get_model::<Note>(turtl, &id)?;
                    Space::permission_check(turtl, &model.space_id, &Permission::EditNote)?;
                    delete_model::<FileData>(turtl, &id, false)?;
                }
                _ => {
                    return TErr!(TError::BadValue(format!("cannot direct sync an item of type {:?}", ty)));
                }
            }
            Ok(jedi::obj())
        }
        SyncAction::MoveSpace => {
            let item_id = jedi::get(&["id"], &modeldata)?;
            let to_space_id = jedi::get(&["space_id"], &modeldata)?;
            match ty {
                SyncType::Board => {
                    let from_space_id = match Board::get_space_id(turtl, &item_id) {
                        Some(id) => id,
                        None => return TErr!(TError::MissingData(format!("cannot find space id for board {}", item_id))),
                    };
                    Space::permission_check(turtl, &from_space_id, &Permission::DeleteBoard)?;
                    Space::permission_check(turtl, &to_space_id, &Permission::AddBoard)?;
                    let mut profile_guard = turtl.profile.write().unwrap();
                    let boards = &mut profile_guard.boards;
                    let board = match Profile::finder(boards, &item_id) {
                        Some(m) => m,
                        None => return TErr!(TError::MissingData(format!("cannot find Board {} in profile", item_id))),
                    };
                    board.move_spaces(turtl, to_space_id)?;
                }
                SyncType::Note => {
                    let from_space_id = match Note::get_space_id(turtl, &item_id) {
                        Some(id) => id,
                        None => return TErr!(TError::MissingData(format!("cannot find space id for note {}", item_id))),
                    };
                    Space::permission_check(turtl, &from_space_id, &Permission::DeleteNote)?;
                    Space::permission_check(turtl, &to_space_id, &Permission::AddNote)?;
                    let mut notes = turtl.load_notes(&vec![item_id.clone()])?;
                    if notes.len() == 0 {
                        return TErr!(TError::MissingData(format!("trouble grabbing Note {}", item_id)));
                    }
                    let note = &mut notes[0];
                    note.move_spaces(turtl, to_space_id)?;
                }
                _ => {
                    return TErr!(TError::BadValue(format!("cannot {:?} item of type {:?}", action, ty)));
                }
            }
            Ok(jedi::obj())
        }
        _ => {
            TErr!(TError::BadValue(format!("unimplemented sync action {:?}", action)))
        }
    }
}

