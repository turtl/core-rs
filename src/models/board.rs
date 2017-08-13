use ::jedi::Value;

use ::error::TResult;
use ::crypto::Key;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::models::note::Note;
use ::models::keychain::{Keychain, KeyRef};
use ::turtl::Turtl;
use ::sync::sync_model::{self, SyncModel, MemorySaver};

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Board {
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,
        #[protected_field(public)]
        pub space_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub meta: Option<Value>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub title: Option<String>,
    }
}

make_storable!(Board, "boards");
impl SyncModel for Board {}

impl Keyfinder for Board {
    fn get_key_search(&self, turtl: &Turtl) -> TResult<Keychain> {
        let mut keychain = Keychain::new();
        let mut space_ids: Vec<String> = Vec::new();
        space_ids.push(self.space_id.clone());
        match self.keys.as_ref() {
            Some(keys) => for key in keys {
                match key.get(&String::from("s")) {
                    Some(id) => space_ids.push(id.clone()),
                    None => {},
                }
            },
            None => {},
        }

        if space_ids.len() > 0 {
            let ty = String::from("space");
            let profile_guard = turtl.profile.read().unwrap();
            for space in &profile_guard.spaces {
                if space.id().is_none() || space.key().is_none() { continue; }
                let space_id = space.id().unwrap();
                if !space_ids.contains(space_id) { continue; }
                keychain.upsert_key(turtl, space_id, space.key().unwrap(), &ty)?;
            }
        }
        Ok(keychain)
    }

    fn get_keyrefs(&self, turtl: &Turtl) -> TResult<Vec<KeyRef<Key>>> {
        let mut refs: Vec<KeyRef<Key>> = Vec::new();
        let profile_guard = turtl.profile.read().unwrap();
        for space in &profile_guard.spaces {
            if space.id() == Some(&self.space_id) && space.key().is_some() {
                refs.push(KeyRef {
                    id: self.space_id.clone(),
                    ty: String::from("s"),
                    k: space.key().unwrap().clone(),
                });
            }
        }
        Ok(refs)
    }
}

impl MemorySaver for Board {
    fn save_to_mem(self, turtl: &Turtl) -> TResult<()> {
        let mut profile_guard = turtl.profile.write().unwrap();
        for board in &mut profile_guard.boards {
            if board.id() == self.id() {
                board.merge_fields(&self.data()?)?;
                return Ok(())
            }
        }
        // if it doesn't exist, push it on
        profile_guard.boards.push(self);
        Ok(())
    }

    fn delete_from_mem(&self, turtl: &Turtl) -> TResult<()> {
        let mut profile_guard = turtl.profile.write().unwrap();
        let board_id = self.id().unwrap();

        let notes: Vec<Note> = {
            let db_guard = turtl.db.read().unwrap();
            match *db_guard {
                Some(ref db) => db.find("notes", "board_id", &vec![board_id.clone()])?,
                None => vec![],
            }
        };
        for note in notes {
            let note_id = match note.id() {
                Some(x) => x,
                None => {
                    warn!("Board.delete_from_mem() -- got a note from the local DB with empty `id` field");
                    continue;
                }
            };
            sync_model::delete_model::<Note>(turtl, &note_id, true)?;
        }
        // remove the board from memory
        profile_guard.boards.retain(|b| b.id() != Some(&board_id));
        Ok(())
    }
}

