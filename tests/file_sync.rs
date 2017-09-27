include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn file_sync() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy@turtlapp.com", password]));
        dispatch_ass(json!(["sync:start"]));

        wait_on("profile:loaded");
        wait_on("profile:indexed");

        let profile_data = dispatch_ass(json!(["profile:load"]));

        let user_id: String = jedi::get(&["user", "id"], &profile_data).unwrap();
        let space_id: String = jedi::get(&["spaces", "0", "id"], &profile_data).unwrap();
        let notejson = &json!({
            "title": "mai file LOL",
            "space_id": space_id,
            "user_id": user_id,
            "file": {
                "name": "slappy.json",
                "type": "application/json",
                "filedata": {
                    "data": String::from(r#"eyJuYW1lIjoic2xhcHB5IiwibG9jYXRpb24iOnsiY2l0eSI6InNhbnRhIGNydXp6eiJ9LCJlbmpveXMiOiJzaGFyaW5nIHNlbGZpZXMgd2l0aCBtYWkgaW5zdGFncmFtIGZvbGxvd2VycyEiLCJpbnN0YWdyYW1fZm9sbG93ZXJzIjpbXX0="#),
                },
            }
        });
        let res = dispatch(json!([
            "profile:sync:model",
            "add",
            "note",
            notejson,
        ]));
        let note_id: String = jedi::get(&["id"], &res.d).unwrap();
        if res.e != 0 {
            panic!("bad response from profile:sync:model -- {:?}", res);
        }

        wait_on("sync:file:uploaded");
        dispatch_ass(json!(["app:wipe-user-data"]));
        wait_on("user:logout");

        sleep(500);

        dispatch_ass(json!(["user:login", "slippyslappy@turtlapp.com", password]));
        dispatch_ass(json!(["sync:start"]));
        let evdata = wait_on("sync:file:downloaded");
        let note_id2: String = jedi::get(&["note_id"], &evdata).unwrap();

        dispatch_ass(json!(["profile:sync:model", "delete", "file", {"id": note_id}]));
        dispatch_ass(json!(["user:delete-account"]));

        assert_eq!(note_id, note_id2);
        end(handle);
    }
}

