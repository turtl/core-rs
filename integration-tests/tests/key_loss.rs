include!("../src/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn key_loss() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy@turtlapp.com", password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");

        // get rid of any sync:outgoing events
        sleep(2000);
        drain_events();

        let profile_res = dispatch(json!(["profile:load"]));

        let user_id: String = jedi::get(&["user", "id"], &profile_res.d).unwrap();
        let default_space_id: String = jedi::get(&["user", "settings", "default_space"], &profile_res.d).unwrap();
        let spaces: Vec<Value> = jedi::get(&["spaces"], &profile_res.d).unwrap();
        let spaces_not_default: Vec<String> = spaces.iter()
            .map(|x| jedi::get::<String>(&["id"], x).unwrap() )
            .filter(|x| x != &default_space_id)
            .collect::<Vec<_>>();
        let from_space_id = spaces_not_default[0].clone();
        let to_space_id = spaces_not_default[1].clone();

        let mut board = dispatch_ass(json!([
            "profile:sync:model",
            "add",
            "board",
            {"user_id": user_id, "space_id": from_space_id, "title": "he is smart. he makes us strong."},
        ]));
        let board_id: String = jedi::get(&["id"], &board).unwrap();

        // add a "buttload" of notes
        for i in 0..50 {
            // i didn't say buttload.......i said assload
            dispatch_ass(json!([
                "profile:sync:model",
                "add",
                "note",
                {
                    "user_id": user_id,
                    "space_id": from_space_id,
                    "board_id": board_id,
                    "title": format!("get a job {}", i),
                    "body": format!("please break! {}", i),
                }
            ]));
        }
        // note that we'll actually get TWO sync:complete events, since we're
        // adding more notes than the server will allow in a bulk sync (32). so
        // let's wait for the first one, then STRIKE with a board.move_space
        wait_on("sync:outgoing:complete");

        jedi::set::<String>(&["space_id"], &mut board, &to_space_id).unwrap();
        dispatch_ass(json!(["profile:sync:model", "move-space", "board", board]));
        wait_on("sync:outgoing:complete");

        let profile = dispatch_ass(json!(["profile:load"]));
        let search = dispatch_ass(json!(["profile:find-notes", {"space_id": to_space_id}]));
        let num_boards = jedi::get::<Vec<Value>>(&["boards"], &profile).unwrap().len();
        let num_notes = jedi::get::<u32>(&["total"], &search).unwrap();

        // this space is no match for my transmogrifying death ray
        dispatch_ass(json!(["profile:sync:model", "delete", "space", {"id": from_space_id}]));

        fn clear_sync() {
            loop {
                let pending = dispatch_ass(json!(["sync:get-pending"]));
                let pending_syncs: Vec<Value> = jedi::from_val(pending).unwrap();
                if pending_syncs.len() == 0 { break; }
                sleep(1000);
            }
        }

        clear_sync();

        dispatch_ass(json!(["app:wipe-user-data"]));

        dispatch_ass(json!(["user:login", "slippyslappy@turtlapp.com", password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");

        let profile = dispatch_ass(json!(["profile:load"]));
        let search = dispatch_ass(json!(["profile:find-notes", {"space_id": to_space_id}]));

        dispatch_ass(json!(["user:delete-account"]));

        let new_num_boards = jedi::get::<Vec<Value>>(&["boards"], &profile).unwrap().len();
        let new_num_notes = jedi::get::<u32>(&["total"], &search).unwrap();

        assert_eq!(num_boards, new_num_boards);
        assert_eq!(num_notes, new_num_notes);

        end(handle);
    }
}


