include!("../src/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn user_private_key_loss() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy+losemykey@turtlapp.com", password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");

        // get rid of any sync:outgoing events
        sleep(2000);
        drain_events();

        dispatch_ass(json!([
            "profile:sync:model",
            "edit",
            "user",
            {"settings": {"your_mom": true}},
        ]));

        let profile_res = dispatch(json!(["profile:load"]));
        let user_privkey: Option<String> = jedi::get_opt(&["user", "privkey"], &profile_res.d);

        dispatch_ass(json!(["user:delete-account"]));
        assert!(user_privkey.is_some());
        end(handle);
    }
}


