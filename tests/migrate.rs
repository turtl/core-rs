include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn migrates() {
        let handle = init();
        let old_username = config::get::<String>(&["integration_tests", "v6_login", "username"]).unwrap();
        let old_password = config::get::<String>(&["integration_tests", "v6_login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));

        let res = dispatch_ass(json!(["user:can-migrate", old_username, old_password]));
        let works: bool = jedi::from_val(res).unwrap();
        assert!(works);

        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();
        let new_password = format!("{}_newLOLOL", password);

        dispatch_ass(json!(["user:join-migrate", old_username, old_password, "slippyslappy@turtlapp.com", new_password]));
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");
        let profile_data = dispatch_ass(json!(["profile:load"]));
        #[derive(Deserialize, Clone)]
        struct Space {
            id: String,
            title: String,
        }
        #[derive(Deserialize)]
        struct Profile {
            spaces: Vec<Space>,
        }
        let profile: Profile = jedi::from_val(profile_data).unwrap();
        let mut migrate_space = None;
        for space in &profile.spaces {
            if space.title == "Imported" {
                migrate_space = Some(space);
            }
        }
        let migrate_space = migrate_space.unwrap();
        let notes = dispatch_ass(json!(["profile:find-notes", {"space_id": migrate_space.id, "sort": "id"}]));
        wait_on("sync:outgoing:complete");
        dispatch_ass(json!(["user:delete-account"]));
        end(handle);

        let notes: Vec<Value> = jedi::get(&["notes"], &notes).unwrap();
        assert!(notes.len() > 0);
    }
}


