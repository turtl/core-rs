extern crate jedi;

include!("./_lib.rs");

use jedi::Value;

#[cfg(test)]
mod tests {
    use super::*;

    use ::std::thread;
    use ::carrier;
    use ::config;

    fn end(handle: thread::JoinHandle<()>) {
        send(r#"["4269","app:shutdown"]"#);
        handle.join().unwrap();
        carrier::wipe();
    }

    #[test]
    fn join_delete_account() {
        let handle = init();
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
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
        while recv_event() != r#"{"e":"profile:loaded","d":{}}"# {}
        // wait until we're indexed
        while recv_event() != r#"{"e":"profile:indexed","d":{}}"# {}

        let msg = format!(r#"["30","profile:load"]"#);
        send(msg.as_str());
        // load the profile json for later
        let profile_json = recv("30");
        sleep(10);

        let msg = format!(r#"["3","user:delete-account","{}","{}"]"#, username, password);
        send(msg.as_str());
        let msg = recv("3");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);
        end(handle);

        // verify our profile AFTER the account is deleted. this keeps profile
        // assert failures from making me have to delete the user by hand on
        // each test run
        let val: Value = jedi::parse(&profile_json).unwrap();
        let data: Value = jedi::get(&["d"], &val).unwrap();
        let spaces: Vec<Value> = jedi::get(&["spaces"], &data).unwrap();
        let boards: Vec<Value> = jedi::get(&["boards"], &data).unwrap();
        let ptitle: String = jedi::get(&["spaces", "0", "title"], &data).unwrap();
        assert_eq!(spaces.len(), 3);
        assert_eq!(boards.len(), 3);
        assert_eq!(ptitle, "Personal");
    }
}

