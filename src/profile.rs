//! The Profile module exports a struct that is responsible for handling and
//! storing the user's data (keychain, boards, etc) in-memory.
//!
//! It only stores data for the keychain, persona (soon deprecated), and boards
//! (so no note data). The reason is that keychain/boards are useful to keep in
//! memory to decrypt notes, but otherwise, notes can just be loaded on the fly
//! from local storage and discarded once sent to the UI.

use ::std::collections::HashMap;
use ::turtl::Turtl;
use ::error::{TResult, TError};
use ::jedi::{self, Value};
use ::models::model::{self, Model};
use ::models::keychain::Keychain;
use ::models::space::Space;
use ::models::board::Board;
use ::models::note::Note;
use ::models::file::FileData;
use ::models::invite::Invite;
use ::models::protected::{self, Protected};
use ::models::sync_record::{SyncRecord, SyncAction, SyncType};
use ::models::storable::Storable;
use ::sync::sync_model;
use ::lib_permissions::Permission;
use ::config;
use ::crypto;
use ::messaging;

/// A structure holding a collection of objects that represent's a user's
/// Turtl data profile.
pub struct Profile {
    pub keychain: Keychain,
    pub spaces: Vec<Space>,
    pub boards: Vec<Board>,
    pub invites: Vec<Invite>,
}

/// A struct for holding a profile export
#[derive(Serialize, Deserialize, Default)]
pub struct Export {
    schema_version: u16,
    spaces: Vec<Space>,
    boards: Vec<Board>,
    notes: Vec<Note>,
    files: Vec<FileData>,
}

/// Holds the result of an import
#[derive(Serialize, Default)]
pub struct ImportResult {
    actions: Vec<SyncRecord>,
}

/// This lets us know how an import should be processed.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum ImportMode {
    /// Only import items missing from the current profile
    #[serde(rename = "restore")]
    Restore,
    /// Import everything, overwriting existing items
    #[serde(rename = "replace")]
    Replace,
    /// Completely wipe current profile before importing
    #[serde(rename = "full")]
    Full,
}

impl Profile {
    pub fn new() -> Profile {
        Profile {
            keychain: Keychain::new(),
            spaces: Vec::new(),
            boards: Vec::new(),
            invites: Vec::new(),
        }
    }

    /// Wipe the profile from memory
    pub fn wipe(&mut self) {
        self.keychain = Keychain::new();
        self.spaces = Vec::new();
        self.boards = Vec::new();
        self.invites = Vec::new();
    }

    /// Find a model by id in a collection of items
    pub fn finder<'a, T>(items: &'a mut Vec<T>, item_id: &String) -> Option<&'a mut T>
        where T: Model
    {
        items.iter_mut()
            .filter(|x| x.id() == Some(item_id))
            .next()
    }

    /// Export the current Turtl profile
    pub fn export(turtl: &Turtl) -> TResult<Export> {
        info!("Profile::export() -- running export");
        let mut export = Export::default();
        export.schema_version = 2;
        let profile_guard = lockr!(turtl.profile);
        let mut db_guard = lock!(turtl.db);
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(String::from("turtl.db"))),
        };
        fn cloner<T: Protected>(models: &Vec<T>) -> TResult<Vec<T>> {
            let mut res = Vec::with_capacity(models.len());
            for model in models {
                let mut newmodel = model.clone()?;
                newmodel.clear_body();
                newmodel.set_keys(Vec::new());
                res.push(newmodel);
            }
            Ok(res)
        }
        export.spaces = cloner(&profile_guard.spaces)?
            .into_iter()
            .map(|mut x| {
                x.members = Vec::new();
                x.invites = Vec::new();
                x
            })
            .collect::<Vec<_>>();
        export.boards = cloner(&profile_guard.boards)?;
        let mut notes_encrypted = db.all(Note::tablename())?;
        turtl.find_models_keys(&mut notes_encrypted)?;
        export.notes = protected::map_deserialize(turtl, notes_encrypted)?;
        export.files = Vec::with_capacity(export.notes.len());
        for note in &export.notes {
            match FileData::load_file(turtl, note) {
                Ok(binary) => {
                    let mut filedata = FileData::default();
                    filedata.set_id(note.id_or_else()?);
                    filedata.data = Some(binary);
                    export.files.push(filedata);
                }
                Err(_) => {}    // we beleeze in nuzzing, lebowzki.
            }
        }
        Ok(export)
    }

    /// Import a dump into the current Turtl profile.
    ///
    /// If an item is added (as opposed to editing an existing model), it's
    /// important to note that the model's ID is regenerated before saving and
    /// its old id is added to a hash that maps old id -> new id. Then any model
    /// that references the old id will have that reference updated to the new
    /// id. This can be done in one pass since the references are hierarchical,
    /// luckily. Also, we don't have to update key references because those are
    /// fully regenerated on each save >=]
    pub fn import(turtl: &Turtl, mode: ImportMode, export: Export) -> TResult<ImportResult> {
        let client_id = {
            let key = format!("{}/{}", config::get::<String>(&["api", "endpoint"])?, turtl.user_id()?);
            crypto::to_hex(&crypto::sha256(key.as_bytes())?)?
        };
        info!("Profile::import() -- running import (mode: {}, cid: {})", jedi::stringify(&mode)?, client_id);
        // the import result details what changed
        let mut result = ImportResult::default();

        fn simple_sync_action(id: &String, action: SyncAction, ty: SyncType) -> SyncRecord {
            let mut sync_record = SyncRecord::default();
            sync_record.item_id = id.clone();
            sync_record.action = action;
            sync_record.ty = ty;
            sync_record
        }

        if mode == ImportMode::Full {
            // ok, the user has asked us to completely replace their entire
            // profile with the one being imported. kindly oblige them. if we do
            // this by loading turtl.profile and destroying our spaces, we'll
            // deadlock since spaces lock the profile in their MemorySaver
            // impl. instead, we'll just grab them from the local db and wipe
            // them that way.
            //
            // note that by destroying the spaces, we destroy the profile. this
            // includes keychains, boards, notes, etc (etc meaning "actually,
            // that's it" here).
            let spaces: Vec<Space> = {
                let mut db_guard = lock!(turtl.db);
                let db = match db_guard.as_mut() {
                    Some(x) => x,
                    None => return TErr!(TError::MissingField(String::from("turtl.db"))),
                };
                db.all::<Space>(Space::tablename())?
            };
            let user_id = turtl.user_id()?;
            for space in spaces {
                // it would be a bad (read: terrible) idea to remove a space
                // that doesn't belong to us. the API won't let us, and it will
                // end up gumming up the sync system.
                if space.user_id != user_id { continue; }

                let space_id = space.id_or_else()?;

                // another check to make sure we can delete this space.
                match Space::permission_check(turtl, &space_id, &Permission::DeleteSpace) {
                    Ok(_) => {}
                    Err(_) => { continue }
                }

                // kewl, this space belongs to the current user. DESTROY IT!
                sync_model::delete_model::<Space>(turtl, &space_id, false)?;

                // mark it, dude.
                result.actions.push(simple_sync_action(&space_id, SyncAction::Delete, SyncType::Space));
            }
        }

        // ok, now that we got rid of that dead weight, let's start our import.
        let Export { spaces, boards, notes, files, .. } = export;

        struct Counter { count: u32 }
        
        // define a function that runs our sync dispatcher for the incoming
        // import models. note that this runs all of our permission checks for
        // us! yay, abstraction.
        fn saver<T, F>(turtl: &Turtl, mode: &ImportMode, client_id: &String, models: Vec<T>, ty: SyncType, mut ser: F, id_change_map: &mut HashMap<String, String>, result: &mut ImportResult, counter: &mut Counter) -> TResult<()>
            where T: Protected + Storable,
                  F: FnMut(&T, &mut HashMap<String, String>, &String) -> TResult<Value>
        {
            for mut model in models {
                let model_id = model.id_or_else()?;
                let new_id = model::cid_w_client_id(&model_id, &client_id)?;
                let (id, exists) = {
                    let mut db_guard = lock!(turtl.db);
                    let db = match db_guard.as_mut() {
                        Some(x) => x,
                        None => return TErr!(TError::MissingField(String::from("turtl.db"))),
                    };
                    if db.get::<T>(T::tablename(), &model_id)?.is_some() {
                        (model_id.clone(), true)
                    } else if db.get::<T>(T::tablename(), &new_id)?.is_some() {
                        (new_id.clone(), true)
                    } else {
                        (new_id.clone(), false)
                    }
                };
                model.set_id(id.clone());
                if id == new_id {
                    id_change_map.insert(model_id.clone(), new_id.clone());
                }
                let mut sync_record = SyncRecord::default();
                sync_record.ty = ty.clone();
                sync_record.data = Some(ser(&model, id_change_map, &model_id)?);
                if exists {
                    // if the model already exists and we're only loading
                    // missing items, skip importing this model
                    if mode == &ImportMode::Restore { continue; }
                    sync_record.action = SyncAction::Edit;
                } else {
                    sync_record.action = SyncAction::Add;
                }
                info!("Profile::import() -- import: {}/{}/{}", jedi::stringify(&sync_record.action)?, jedi::stringify(&sync_record.ty)?, id);
                result.actions.push(simple_sync_action(&id, sync_record.action.clone(), sync_record.ty.clone()));
                sync_model::dispatch(turtl, sync_record)?;
                // tally ho, good chap
                counter.count += 1;
                messaging::ui_event("profile:import:tally", &counter.count)?;
            }
            Ok(())
        }

        let mut id_change_map: HashMap<String, String> = HashMap::new();
        let mut file_idx: HashMap<String, FileData> = HashMap::new();
        for file in files {
            file_idx.insert(file.id_or_else()?, file);
        }

        /// Check if we have replaced an old id with a newly generated one and,
        /// if so, switches that id out in the data at the given key.
        fn switch_id_if_needed(id_change_map: &mut HashMap<String, String>, data: &mut Value, key: &str) -> TResult<()> {
            match jedi::get_opt::<String>(&[key], &data) {
                Some(old_id) => {
                    // grab the new id (if it exists) otherwise the old id
                    // instead
                    let new_id = id_change_map.get(&old_id)
                        .map(|x| x.clone())
                        .unwrap_or(old_id);
                    // set the new id back into the data
                    jedi::set(&[key], data, &new_id)?;
                }
                None => {}
            }
            Ok(())
        }

        let mut counter = Counter { count: 0 };
        saver(turtl, &mode, &client_id, spaces, SyncType::Space, |x, _map, _old_id| { x.data() }, &mut id_change_map, &mut result, &mut counter)?;
        saver(turtl, &mode, &client_id, boards, SyncType::Board, |x, id_change_map, _old_id| {
            let mut data = x.data()?;
            switch_id_if_needed(id_change_map, &mut data, "space_id")?;
            Ok(data)
        }, &mut id_change_map, &mut result, &mut counter)?;
        saver(turtl, &mode, &client_id, notes, SyncType::Note, |x, id_change_map, old_id| {
            let mut data = x.data()?;
            switch_id_if_needed(id_change_map, &mut data, "space_id")?;
            switch_id_if_needed(id_change_map, &mut data, "board_id")?;
            // it's important to note: at this point, the note's id has not been
            // changed/added to id_change_map, so we don't need to check it
            // against id_change_map when grabbing the note id
            let new_note_id = x.id_or_else()?;

            if let Some(filedata) = file_idx.remove(old_id) {
                // NOTE: no need to set/remove `file.id` here since it will be
                // set when the note is saved.
                jedi::set(&["file", "filedata"], &mut data, &filedata)?;
            }
            Ok(data)
        }, &mut id_change_map, &mut result, &mut counter)?;
        Ok(result)
    }
}

