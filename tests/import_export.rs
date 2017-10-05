include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::config;

    #[test]
    fn import_export() {
        let handle = init();
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:login", username, password]));
        dispatch_ass(json!(["sync:start"]));

        wait_on("profile:loaded");
        wait_on("profile:indexed");
        wait_on("sync:file:downloaded");

        // wait for sync to complete
        sleep(1000);

        let export = dispatch_ass(json!(["profile:export"]));
        let spaces: Vec<Value> = jedi::get(&["spaces"], &export).unwrap();
        let boards: Vec<Value> = jedi::get(&["boards"], &export).unwrap();
        let notes: Vec<Value> = jedi::get(&["notes"], &export).unwrap();
        let files: Vec<Value> = jedi::get(&["files"], &export).unwrap();

        assert_eq!(spaces.len(), 3);
        assert_eq!(boards.len(), 3);
        assert_eq!(notes.len(), 5);
        assert_eq!(files.len(), 1);

        // kewl, got our export. now create a new account and run the import in
        // various modes.

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy@turtlapp.com", password]));
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");

        #[derive(Deserialize)]
        struct SyncRecord {
            #[allow(dead_code)]
            item_id: String,
            action: String,
            #[serde(rename = "type")]
            ty: String
        }
        #[derive(Deserialize)]
        struct ImportResult {
            actions: Vec<SyncRecord>,
        }

        #[derive(Default)]
        struct Breakdown {
            space_add: i32,
            space_edit: i32,
            space_delete: i32,
            board_add: i32,
            board_edit: i32,
            board_delete: i32,
            note_add: i32,
            note_edit: i32,
            note_delete: i32,
        }

        #[derive(Deserialize)]
        struct Profile {
            spaces: Vec<Value>,
            boards: Vec<Value>,
        }

        // convert an import value result into a {type-action: count, ...} hash
        fn import_to_breakdown(import: Value) -> Breakdown {
            let result: ImportResult = jedi::from_val(import).unwrap();
            let mut breakdown = Breakdown::default();
            for rec in &result.actions {
                match rec.ty.as_ref() {
                    "space" => {
                        match rec.action.as_ref() {
                            "add" => { breakdown.space_add += 1; }
                            "edit" => { breakdown.space_edit += 1; }
                            "delete" => { breakdown.space_delete += 1; }
                            _ => {}
                        }
                    }
                    "board" => {
                        match rec.action.as_ref() {
                            "add" => { breakdown.board_add += 1; }
                            "edit" => { breakdown.board_edit += 1; }
                            "delete" => { breakdown.board_delete += 1; }
                            _ => {}
                        }
                    }
                    "note" => {
                        match rec.action.as_ref() {
                            "add" => { breakdown.note_add += 1; }
                            "edit" => { breakdown.note_edit += 1; }
                            "delete" => { breakdown.note_delete += 1; }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            breakdown
        }

        // let's just load all of our results beforehand, so we can delete our
        // temp user before all of our asserts failwhale.
        let profile0: Profile = jedi::from_val(dispatch_ass(json!(["profile:load"]))).unwrap();
        let import1 = dispatch_ass(json!(["profile:import", "restore", export]));
        let profile1: Profile = jedi::from_val(dispatch_ass(json!(["profile:load"]))).unwrap();
        let import2 = dispatch_ass(json!(["profile:import", "restore", export]));
        let profile2: Profile = jedi::from_val(dispatch_ass(json!(["profile:load"]))).unwrap();
        let import3 = dispatch_ass(json!(["profile:import", "replace", export]));
        let profile3: Profile = jedi::from_val(dispatch_ass(json!(["profile:load"]))).unwrap();
        let import4 = dispatch_ass(json!(["profile:import", "full", export]));
        let profile4: Profile = jedi::from_val(dispatch_ass(json!(["profile:load"]))).unwrap();
        // goodbyyye, misterrrrrrr aaaandersonnnnn
        dispatch_ass(json!(["user:delete-account"]));

        assert_eq!(profile0.spaces.len(), 3);
        assert_eq!(profile0.boards.len(), 3);

        let breakdown1 = import_to_breakdown(import1);
        assert_eq!(breakdown1.space_add, 3);
        assert_eq!(breakdown1.space_edit, 0);
        assert_eq!(breakdown1.space_delete, 0);
        assert_eq!(breakdown1.board_add, 3);
        assert_eq!(breakdown1.board_edit, 0);
        assert_eq!(breakdown1.board_delete, 0);
        assert_eq!(breakdown1.note_add, 5);
        assert_eq!(breakdown1.note_edit, 0);
        assert_eq!(breakdown1.note_delete, 0);
        assert_eq!(profile1.spaces.len(), 6);
        assert_eq!(profile1.boards.len(), 6);

        let breakdown2 = import_to_breakdown(import2);
        assert_eq!(breakdown2.space_add, 0);
        assert_eq!(breakdown2.space_edit, 0);
        assert_eq!(breakdown2.space_delete, 0);
        assert_eq!(breakdown2.board_add, 0);
        assert_eq!(breakdown2.board_edit, 0);
        assert_eq!(breakdown2.board_delete, 0);
        assert_eq!(breakdown2.note_add, 0);
        assert_eq!(breakdown2.note_edit, 0);
        assert_eq!(breakdown2.note_delete, 0);
        assert_eq!(profile2.spaces.len(), 6);
        assert_eq!(profile2.boards.len(), 6);

        let breakdown3 = import_to_breakdown(import3);
        assert_eq!(breakdown3.space_add, 0);
        assert_eq!(breakdown3.space_edit, 3);
        assert_eq!(breakdown3.space_delete, 0);
        assert_eq!(breakdown3.board_add, 0);
        assert_eq!(breakdown3.board_edit, 3);
        assert_eq!(breakdown3.board_delete, 0);
        assert_eq!(breakdown3.note_add, 0);
        assert_eq!(breakdown3.note_edit, 5);
        assert_eq!(breakdown3.note_delete, 0);
        assert_eq!(profile3.spaces.len(), 6);
        assert_eq!(profile3.boards.len(), 6);

        let breakdown4 = import_to_breakdown(import4);
        assert_eq!(breakdown4.space_add, 3);
        assert_eq!(breakdown4.space_edit, 0);
        assert_eq!(breakdown4.space_delete, 6);
        assert_eq!(breakdown4.board_add, 3);
        assert_eq!(breakdown4.board_edit, 0);
        assert_eq!(breakdown4.board_delete, 0);
        assert_eq!(breakdown4.note_add, 5);
        assert_eq!(breakdown4.note_edit, 0);
        assert_eq!(breakdown4.note_delete, 0);
        assert_eq!(profile4.spaces.len(), 3);
        assert_eq!(profile4.boards.len(), 3);

        end(handle);
    }
}

