#[macro_use]
extern crate serde_json;

include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::config;

    #[test]
    fn filesync_outgoing() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        let msg = format!(r#"["69","app:wipe-app-data"]"#);
        send(msg.as_str());
        let msg = recv("69");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = format!(r#"["2","user:join","{}","{}"]"#, "slippyslappy@turtlapp.com", password);
        send(msg.as_str());
        let msg = recv("2");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["4","sync:start"]"#);
        send(msg.as_str());
        let msg = recv("4");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        // wait for sync to complete. note we do this before the events fire.
        // this is fine, because they queue.
        sleep(1000);

        // wait until we're loaded
        wait_on("profile:loaded");
        // wait until we're indexed
        wait_on("profile:indexed");

        let msg = format!(r#"["30","profile:load"]"#);
        send(msg.as_str());
        // load the profile json for later
        let profile_json = recv("30");
        sleep(10);

        let val: Value = jedi::parse(&profile_json).unwrap();
        let data: Value = jedi::get(&["d"], &val).unwrap();
        let user_id: String = jedi::get(&["user", "id"], &data).unwrap();
        let space_id: String = jedi::get(&["spaces", "0", "id"], &data).unwrap();
        let notejson = &json!({
            "title": "mai file LOL",
            "space_id": space_id,
            "user_id": user_id,
            "file": {
                "name": "slappy.json",
                "type": "application/json"
            }
        });
        let file_contents = String::from(r#"eyJuYW1lIjoic2xhcHB5IiwibG9jYXRpb24iOnsiY2l0eSI6InNhbnRhIGNydXp6eiJ9LCJlbmpveXMiOiJzaGFyaW5nIHNlbGZpZXMgd2l0aCBtYWkgaW5zdGFncmFtIGZvbGxvd2VycyEiLCJpbnN0YWdyYW1fZm9sbG93ZXJzIjpbXX0="#);
        let filejson = &json!({
            "data": file_contents,
        });
        let msg = jedi::stringify(&json!([
            "8",
            "profile:sync:model",
            "add",
            "note",
            notejson,
            filejson,
        ])).unwrap();
        send(msg.as_str());
        let msg = recv("8");
        let parsed: Value = jedi::parse(&msg).unwrap();
        let note_id: String = jedi::get(&["d", "id"], &parsed).unwrap();
        if jedi::get::<i64>(&["e"], &parsed).unwrap() != 0 {
            panic!("bad response from profile:sync:model -- {}", msg);
        }

        wait_on("sync:file:uploaded");

        let msg = jedi::stringify(&json!([
            "9",
            "profile:sync:model",
            "delete",
            "file",
            {"id": note_id},
        ])).unwrap();
        send(msg.as_str());
        let msg = recv("9");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);

        sleep(1000);
        let msg = format!(r#"["3","user:delete-account"]"#);
        send(msg.as_str());
        let msg = recv("3");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);
        end(handle);
    }
}

