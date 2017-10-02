include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::std::collections::HashMap;
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

        // convert an import value result into a {type-action: count, ...} hash
        fn import_to_breakdown(import: Value) -> HashMap<String, i32> {
            let result: ImportResult = jedi::from_val(import).unwrap();
            let mut breakdown: HashMap<String, i32> = HashMap::new();
            for rec in &result.actions {
                let key = format!("{}-{}", rec.ty, rec.action);
                let counter = breakdown.entry(key).or_insert(0);
                *counter += 1;
            }
            breakdown
        }

        // let's just load all of our results beforehand, so we can delete our
        // temp user before all of our asserts failwhale.
        let profile0 = dispatch_ass(json!(["profile:load"]));
        let import1 = dispatch_ass(json!(["profile:import", "restore", export]));
        let profile1 = dispatch_ass(json!(["profile:load"]));
        let import2 = dispatch_ass(json!(["profile:import", "restore", export]));
        let profile2 = dispatch_ass(json!(["profile:load"]));
        let import3 = dispatch_ass(json!(["profile:import", "replace", export]));
        let profile3 = dispatch_ass(json!(["profile:load"]));
        let import4 = dispatch_ass(json!(["profile:import", "full", export]));
        let profile4 = dispatch_ass(json!(["profile:load"]));
        // goodbyyye, misterrrrrrr aaaandersonnnnn
        dispatch_ass(json!(["user:delete-account"]));

        let breakdown1 = import_to_breakdown(import1);
        assert_eq!(breakdown1.get(&String::from("space-add")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("space-edit")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("space-delete")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("board-add")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("board-edit")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("board-delete")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("note-add")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("note-edit")).unwrap_or(&0), &0);
        assert_eq!(breakdown1.get(&String::from("note-delete")).unwrap_or(&0), &0);

        end(handle);
    }
}

