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
        let profile_res = dispatch(json!(["profile:load"]));
        let notes = dispatch(json!(["profile:find-notes", {"sort": "id"}]));
        wait_on("sync:outgoing:complete");
        dispatch_ass(json!(["user:delete-account"]));
        end(handle);
    }
}


