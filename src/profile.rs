//! The Profile module exports a struct that is responsible for handling and
//! storing the user's data (keychain, boards, etc) in-memory.
//!
//! It only stores data for the keychain, persona (soon deprecated), and boards
//! (so no note data). The reason is that keychain/boards are useful to keep in
//! memory to decrypt notes, but otherwise, notes can just be loaded on the fly
//! from local storage and discarded once sent to the UI.

use ::turtl::Turtl;
use ::error::{TResult, TError};
use ::jedi::Value;
use ::models::model::Model;
use ::models::keychain::Keychain;
use ::models::space::Space;
use ::models::board::Board;
use ::models::note::Note;
use ::models::file::FileData;
use ::models::invite::Invite;
use ::models::protected::{self, Protected};
use ::models::storable::Storable;

/// A structure holding a collection of objects that represent's a user's
/// Turtl data profile.
pub struct Profile {
    pub keychain: Keychain,
    pub spaces: Vec<Space>,
    pub boards: Vec<Board>,
    pub invites: Vec<Invite>,
}

/// This lets us know how an import should be processed.
#[derive(Deserialize)]
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
    pub fn export(turtl: &Turtl) -> TResult<Value> {
        let schema_version = 1;
        let profile_guard = turtl.profile.read().unwrap();
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return TErr!(TError::MissingField(String::from("turtl.db"))),
        };
        fn to_data<T: Protected>(items: &Vec<T>) -> TResult<Vec<Value>> {
            let mut res: Vec<Value> = Vec::with_capacity(items.len());
            for x in items {
                res.push(x.data()?);
            }
            Ok(res)
        }
        let keychain = to_data(&profile_guard.keychain.entries)?;
        let spaces = to_data(&profile_guard.spaces)?;
        let boards = to_data(&profile_guard.boards)?;
        let mut notes_encrypted = db.all(Note::tablename())?;
        turtl.find_models_keys(&mut notes_encrypted)?;
        let notes = protected::map_deserialize(turtl, notes_encrypted)?;
        let mut files = Vec::with_capacity(notes.len());
        for note in &notes {
            match FileData::load_file(turtl, note) {
                Ok(binary) => {
                    let mut filedata = FileData::default();
                    filedata.data = Some(binary);
                    files.push(json!({
                        "note_id": note.id(),
                        "filedata": filedata,
                    }));
                }
                Err(_) => {}    // we beleeze in nuzzing, lebowzki.
            }
        }
        Ok(json!({
            "schema": schema_version,
            "keychain": keychain,
            "spaces": spaces,
            "boards": boards,
            "notes": notes,
            "files": files,
        }))
    }

    /// Import a dump into the current Turtl profile
    pub fn import(turtl: &Turtl, mode: ImportMode, dump: Value) -> TResult<()> {
        Ok(())
    }
}

