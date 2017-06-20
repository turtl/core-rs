use ::jedi;
use ::turtl::Turtl;
use ::error::TResult;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::models::keychain::Keychain;
use ::models::file::File;
use ::sync::item::SyncItem;

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Note {
        #[protected_field(public)]
        space_id: String,
        #[protected_field(public)]
        board_id: Option<String>,
        #[protected_field(public)]
        user_id: String,
        #[protected_field(public, submodel)]
        file: Option<File>,
        #[protected_field(public)]
        has_file: bool,
        #[serde(rename = "mod")]
        #[protected_field(public)]
        mod_: i64,

        #[serde(rename = "type")]
        #[protected_field(private)]
        type_: Option<String>,
        #[protected_field(private)]
        title: Option<String>,
        #[protected_field(private)]
        tags: Option<Vec<String>>,
        #[protected_field(private)]
        url: Option<String>,
        #[protected_field(private)]
        username: Option<String>,
        #[protected_field(private)]
        password: Option<String>,
        #[protected_field(private)]
        text: Option<String>,
        #[protected_field(private)]
        embed: Option<String>,
        #[protected_field(private)]
        color: Option<i64>,
    }
}

make_storable!(Note, "notes");
make_basic_sync_model!{ Note,
    fn transform(&self, mut sync_item: SyncItem) -> TResult<SyncItem> {
        let data = sync_item.data.as_ref().unwrap().clone();
        match jedi::get::<String>(&["board_id"], &data) {
            Ok(board_id) => {
                jedi::set(&["boards"], sync_item.data.as_mut().unwrap(), &vec![board_id])?;
            },
            Err(_) => {},
        }

        if jedi::get_opt::<String>(&["file", "hash"], &data).is_some() {
            jedi::set(&["file", "id"], sync_item.data.as_mut().unwrap(), &jedi::get::<String>(&["file", "hash"], &data)?)?;
        }

        Ok(sync_item)
    }
}

impl Keyfinder for Note {
    fn get_key_search(&self, turtl: &Turtl) -> Keychain {
        let mut keychain = Keychain::new();
        let mut space_ids: Vec<String> = Vec::new();
        let mut board_ids: Vec<String> = Vec::new();
        if self.space_id.is_some() {
            space_ids.push(self.space_id.as_ref().unwrap().clone());
        }
        if self.board_id.is_some() {
            board_ids.push(self.board_id.as_ref().unwrap().clone());
        }
        match self.get_keys() {
            Some(keys) => for key in keys {
                match key.get(&String::from("s")) {
                    Some(id) => space_ids.push(id.clone()),
                    None => {},
                }
                match key.get(&String::from("b")) {
                    Some(id) => board_ids.push(id.clone()),
                    None => {},
                }
            },
            None => {},
        }

        let user_id = String::from("");     // fake id is ok
        if space_ids.len() > 0 {
            let ty = String::from("space");
            let profile_guard = turtl.profile.read().unwrap();
            for space in &profile_guard.spaces {
                if space.id().is_none() || space.key().is_none() { continue; }
                let space_id = space.id().unwrap();
                if !space_ids.contains(space_id) { continue; }
                keychain.add_key(&user_id, space_id, space.key().unwrap(), &ty);
            }
        }
        if board_ids.len() > 0 {
            let ty = String::from("board");
            let profile_guard = turtl.profile.read().unwrap();
            for board in &profile_guard.boards {
                if board.id().is_none() || board.key().is_none() { continue; }
                let board_id = board.id().unwrap();
                if !board_ids.contains(board_id) { continue; }
                keychain.add_key(&user_id, board_id, board.key().unwrap(), &ty);
            }
        }
        keychain
    }
}

