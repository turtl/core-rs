include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    #[test]
    fn invites() {
        let handle = init();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:join", "slippyslappy@turtlapp.com", password]));
        dispatch_ass(json!(["sync:start"]));

        wait_on("profile:loaded");
        wait_on("profile:indexed");

        let profile = dispatch_ass(json!(["profile:load"]));

        let to_user_email: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let to_user = dispatch_ass(json!(["user:find-by-email", to_user_email]));
        let pubkey: String = jedi::get(&["pubkey"], &to_user).unwrap();
        let space_id: String = jedi::get(&["spaces", "0", "id"], &profile).unwrap();
        let role = "guest";
        let title = "welcome to my dumb space";
        dispatch_ass(json!(["profile:space:send-invite", {
            "space_id": space_id,
            "to_user": to_user_email,
            "role": role,
            "title": title,
            "their_pubkey": pubkey,
        }]));

        // log out of the created account and log into the invitee's account
        dispatch_ass(json!(["user:logout"]));
        dispatch_ass(json!(["user:login", to_user_email, password]));
        dispatch_ass(json!(["sync:start"]));
        wait_on("profile:loaded");
        wait_on("profile:indexed");
        let profile = dispatch_ass(json!(["profile:load"]));
        let invite: Value = jedi::get(&["invites", "0"], &profile).unwrap();
        let space_id: String = jedi::get(&["invites", "0", "space_id"], &profile).unwrap();
        dispatch_ass(json!(["profile:space:accept-invite", invite]));

        // ok, the space should come through in a sync:update event now, and it
        // should be decrypted (so we check the title, a private field)
        loop {
            let data = wait_on("sync:update");
            let ty: String = jedi::get(&["type"], &data).unwrap();
            // if we got our space, make sure it all checks out
            if ty == "space" {
                let id: String = jedi::get(&["item_id"], &data).unwrap();
                let title: String = jedi::get(&["data", "title"], &data).unwrap();
                assert_eq!(id, space_id);
                assert_eq!(title, "Personal");
                break;
            }
        }

        // now logout and delete our account from above ^^
        dispatch_ass(json!(["user:logout"]));
        dispatch_ass(json!(["user:login", "slippyslappy@turtlapp.com", password]));
        dispatch_ass(json!(["sync:start"]));
        wait_on("sync:connected");
        dispatch_ass(json!(["user:delete-account"]));
        end(handle);
    }
}


