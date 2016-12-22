use ::turtl::Turtl;
use ::error::TResult;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::models::keychain::Keychain;
use ::models::file::File;
use ::sync::item::SyncItem;

protected!{
    pub struct Note {
        ( user_id: String,
          boards: Vec<String>,
          file: File,
          has_file: bool,
          mod_: i64 ),
        ( type_: String,
          title: String,
          tags: Vec<String>,
          url: String,
          username: String,
          password: String,
          text: String,
          embed: String,
          color: i64 ),
        ( ),
        ( file )
    }
}

make_storable!(Note, "notes");
make_basic_sync_model!{ Note,
    fn transform(&self, sync_item: SyncItem) -> TResult<SyncItem> {
        Ok(sync_item)
    }
}

impl Keyfinder for Note {
    fn get_key_search(&self, turtl: &Turtl) -> Keychain {
        let mut keychain = Keychain::new();
        let mut board_ids: Vec<String> = Vec::new();
        match self.boards.as_ref() {
            Some(ids) => for id in ids { board_ids.push(id.clone()); },
            None => {},
        }
        match self.keys.as_ref() {
            Some(keys) => for key in keys {
                match key.get(&String::from("b")) {
                    Some(id) => board_ids.push(id.clone()),
                    None => {},
                }
            },
            None => {},
        }

        let user_id = String::from("");     // fake id is ok
        let ty = String::from("board");
        let profile_guard = turtl.profile.read().unwrap();
        for board in &profile_guard.boards {
            if board.id().is_none() || board.key().is_none() { continue; }
            let board_id = board.id().unwrap();
            if !board_ids.contains(board_id) { continue; }
            keychain.add_key(&user_id, board_id, board.key().unwrap(), &ty);
        }
        keychain
    }
}

