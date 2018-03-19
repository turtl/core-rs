include!("../src/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn join_delete_account() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();
        let new_password = format!("{}_newLOLOL", password);

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy@turtlapp.com", password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));

        wait_on("profile:loaded");
        wait_on("profile:indexed");

        let profile_res = dispatch(json!(["profile:load"]));

        let user_id: String = jedi::get(&["user", "id"], &profile_res.d).unwrap();
        let space_id: String = jedi::get(&["user", "settings", "default_space"], &profile_res.d).unwrap();

        dispatch_ass(json!([
            "profile:sync:model",
            "add",
            "note",
            {
                "user_id": user_id,
                "space_id": space_id,
                "title": "get a job",
                "body": "do it",
            }
        ]));

        dispatch_ass(json!([
            "profile:sync:model",
            "add",
            "note",
            {
                "user_id": user_id,
                "space_id": space_id,
                "title": "my bookmark",
                "url": "http://howtobepunk.com",
            }
        ]));

        // change our password. this will log us out, so we need to log in again
        // to delete the account
        dispatch_ass(json!([
            "user:change-password",
            "slippyslappy@turtlapp.com",
            password,
            "slippyslappy@turtlapp.com",
            new_password,
        ]));

        // log in with our BRAND NEW username/password
        dispatch_ass(json!(["user:login", "slippyslappy@turtlapp.com", new_password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));

        // wait on the profile load. we shouldn't get any errors about bad
        // keychain since we logged in w/ new un/pw
        wait_on("profile:loaded");
        wait_on("profile:indexed");

        let note_search = dispatch_ass(json!([
            "profile:find-notes", {
                "space_id": space_id,
                "url": "http://howtobepunk.com",
            }
        ]));

        dispatch_ass(json!(["user:delete-account"]));
        end(handle);

        let num_notes: u32 = jedi::get(&["total"], &note_search).unwrap();
        assert_eq!(num_notes, 1);

        // verify our profile AFTER the account is deleted. this keeps profile
        // assert failures from making me have to delete the user by hand on
        // each test run
        let spaces: Vec<Value> = jedi::get(&["spaces"], &profile_res.d).unwrap();
        let boards: Vec<Value> = jedi::get(&["boards"], &profile_res.d).unwrap();
        let ptitle: String = jedi::get(&["spaces", "0", "title"], &profile_res.d).unwrap();
        assert!(spaces.len() > 0);
        assert!(boards.len() > 0);
        assert_eq!(ptitle, "Personal");
    }
}

