use ::jedi::Value;

use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::models::keychain::Keychain;
use ::turtl::Turtl;

protected!{
    pub struct Board {
        ( user_id: String,
          parent_id: String,
          privs: Value,
          meta: Value,
          shared: bool ),
        ( title: String ),
        ( )
    }
}

make_storable!(Board, "boards");
make_basic_sync_model!(Board);

impl Keyfinder for Board {
    fn get_key_search(&self, turtl: &Turtl) -> Keychain {
        let mut keychain = Keychain::new();
        let mut board_ids: Vec<String> = Vec::new();
        match self.parent_id.as_ref() {
            Some(id) => board_ids.push(id.clone()),
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

