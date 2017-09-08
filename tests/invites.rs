include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use ::config;

    // login to the temporary (sender) account
    fn login_tmp() -> String {
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();
        dispatch_ass(json!(["app:wipe-user-data"]));
        let ret = dispatch_ass(json!(["user:login", "slippyslappy@turtlapp.com", password]));
        let user_id: String = jedi::from_val(ret).unwrap();
        dispatch_ass(json!(["sync:start"]));
        wait_on("sync:connected");
        wait_on("profile:loaded");
        wait_on("profile:indexed");
        user_id
    }

    fn login_testacct() -> String {
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();
        dispatch_ass(json!(["app:wipe-user-data"]));
        let ret = dispatch_ass(json!(["user:login", username, password]));
        let user_id: String = jedi::from_val(ret).unwrap();
        dispatch_ass(json!(["sync:start"]));
        wait_on("sync:connected");
        wait_on("profile:loaded");
        wait_on("profile:indexed");
        user_id
    }

    // a function we use to join and send an invite to our test user
    fn setup_invite() {
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
    }

    fn load_invite() -> Value {
        let profile = dispatch_ass(json!(["profile:load"]));
        jedi::get(&["invites", "0"], &profile).unwrap()
    }

    fn accept_invite(invite: &Value) -> Value {
        let space_id: String = jedi::get(&["space_id"], invite).unwrap();
        dispatch_ass(json!(["profile:space:accept-invite", invite]));

        // ok, the space should come through in a sync:update event now, and it
        // should be decrypted (so we check the title, a private field)
        loop {
            let data = wait_on("sync:update");
            let ty: String = jedi::get(&["type"], &data).unwrap();
            // if we got our space, make sure it all checks out
            if ty == "space" {
                let id: String = jedi::get(&["item_id"], &data).unwrap();
                if id == space_id {
                    return jedi::get(&["data"], &data).unwrap();
                }
            }
        }
    }

    fn find_member(space: &Value, not_user_id: &String) -> Option<Value> {
        let members: Vec<Value> = jedi::get(&["members"], space).unwrap();
        for mem in members {
            let user_id: String = jedi::get(&["user_id"], &mem).unwrap();
            if &user_id == not_user_id { continue; }
            return Some(mem);
        }
        None
    }

    #[test]
    fn invites() {
        let handle = init();

        // TODO:
        // - edit-invite
        // - delete-invite (as sender)
        // - delete-invite (as recv)

        // test send invite, accept invite, leave space
        setup_invite();
        login_testacct();
        let invite = load_invite();
        let space = accept_invite(&invite);
        let space_id: String = jedi::get(&["id"], &space).unwrap();
        let title: String = jedi::get(&["title"], &space).unwrap();
        assert_eq!(title, "Personal");
        dispatch_ass(json!(["profile:space:leave", space_id]));
        login_tmp();
        dispatch_ass(json!(["user:delete-account"]));


        // test send invite, accept invite, set owner (and set back), edit
        // member, delete member
        setup_invite();
        let test_user_id = login_testacct();
        let invite = load_invite();
        let space = accept_invite(&invite);
        let space_id: String = jedi::get(&["id"], &space).unwrap();
        let tmp_user_id = login_tmp();
        dispatch_ass(json!(["profile:space:set-owner", space_id, test_user_id]));
        wait_on("sync:update");
        login_testacct();
        let space = dispatch_ass(json!(["profile:space:set-owner", space_id, tmp_user_id]));
        wait_on("sync:update");
        login_tmp();
        let mut member = find_member(&space, &tmp_user_id).unwrap();
        jedi::set(&["role"], &mut member, &String::from("admin")).unwrap();
        dispatch_ass(json!(["profile:space:edit-member", member]));
        wait_on("sync:update");
        dispatch_ass(json!(["profile:space:delete-member", space_id, test_user_id]));
        dispatch_ass(json!(["user:delete-account"]));

        end(handle);
    }
}


