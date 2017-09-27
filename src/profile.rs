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
use ::models::model::Model;
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

/// This lets us know how an import should be processed.
#[derive(Deserialize, PartialEq)]
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
        let mut export = Export::default();
        export.schema_version = 1;
        let profile_guard = turtl.profile.read().unwrap();
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(String::from("turtl.db"))),
        };
        fn cloner<T: Protected>(models: &Vec<T>) -> TResult<Vec<T>> {
            let mut res = Vec::with_capacity(models.len());
            for model in models {
                let mut newmodel = model.clone()?;
                newmodel.clear_body();
                res.push(newmodel);
            }
            Ok(res)
        }
        export.spaces = cloner(&profile_guard.spaces)?;
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

    /// Import a dump into the current Turtl profile
    pub fn import(turtl: &Turtl, mode: ImportMode, export: Export) -> TResult<()> {
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
            let mut db_guard = turtl.db.write().unwrap();
            let db = match db_guard.as_mut() {
                Some(x) => x,
                None => return TErr!(TError::MissingField(String::from("turtl.db"))),
            };
            let spaces: Vec<Space> = db.all(Space::tablename())?;
            let user_id = turtl.user_id()?;
            for space in spaces {
                // it would be a bad (read: terrible) idea to remove a space
                // that doesn't belong to us. the API won't let us, and it will
                // end up gumming up the sync system.
                if space.user_id != user_id { continue; }

                // kewl, this space belongs to the current user. DESTROY IT!
                sync_model::delete_model::<Space>(turtl, &space.id_or_else()?, false)?;
            }
        }

        // ok, now that we got rid of that dead weight, let's start our import.
        let Export { spaces, boards, notes, files, .. } = export;
        
        // define a function that runs our sync dispatcher for the incoming
        // import models. note that this runs all of our permission checks for
        // us! yay, abstraction.
        fn saver<T, F>(turtl: &Turtl, mode: &ImportMode, models: Vec<T>, ty: SyncType, mut ser: F) -> TResult<()>
            where T: Protected + Storable,
                  F: FnMut(&T) -> TResult<Value>
        {
            let mut db_guard = turtl.db.write().unwrap();
            let db = match db_guard.as_mut() {
                Some(x) => x,
                None => return TErr!(TError::MissingField(String::from("turtl.db"))),
            };
            for model in models {
                let id = model.id_or_else()?;
                let exists = db.get::<T>(T::tablename(), &id)?.is_some();
                let mut sync_record = SyncRecord::default();
                sync_record.ty = ty.clone();
                sync_record.data = Some(ser(&model)?);
                if exists {
                    // if the space already exists and we're only loading missing
                    // items, skip importing this space
                    if mode == &ImportMode::Restore { continue; }
                    sync_record.action = SyncAction::Edit;
                } else {
                    sync_record.action = SyncAction::Add;
                }
                sync_model::dispatch(turtl, sync_record)?;
            }
            Ok(())
        }

        let mut file_idx: HashMap<String, FileData> = HashMap::new();
        for file in files {
            file_idx.insert(file.id_or_else()?, file);
        }
        saver(turtl, &mode, spaces, SyncType::Space, |x| { x.data() })?;
        saver(turtl, &mode, boards, SyncType::Board, |x| { x.data() })?;
        saver(turtl, &mode, notes, SyncType::Note, |x| {
            let mut data = x.data()?;
            let note_id = x.id_or_else()?;
            if let Some(filedata) = file_idx.remove(&note_id) {
                jedi::set(&["file", "filedata", "data"], &mut data, &filedata)?;
            }
            Ok(data)
        })?;
        Ok(())
    }
}

