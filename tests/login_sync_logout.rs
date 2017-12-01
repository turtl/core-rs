include!("./lib/util.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::config;

    #[test]
    fn login_sync_logout() {
        let handle = init();
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:login", username, password]));
        wait_on("user:login");
        dispatch_ass(json!(["sync:start"]));

        wait_on("profile:loaded");
        wait_on("profile:indexed");
        wait_on("sync:file:downloaded");

        // wait for sync to complete
        sleep(1000);

        let connect = dispatch_ass(json!(["app:connected"]));
        let connected: bool = jedi::from_val(connect).unwrap();
        assert_eq!(connected, true);

        let data = dispatch_ass(json!(["profile:load"]));
        let user_id: String = jedi::get(&["user", "id"], &data).unwrap();
        let spaces: Vec<Value> = jedi::get(&["spaces"], &data).unwrap();
        let boards: Vec<Value> = jedi::get(&["boards"], &data).unwrap();
        let ptitle: String = jedi::get(&["spaces", "0", "title"], &data).unwrap();
        assert!(user_id.len() > 0);
        assert_eq!(spaces.len(), 3);
        assert_eq!(boards.len(), 3);
        assert_eq!(ptitle, "Personal");
        sleep(10);

        let noteval = dispatch_ass(json!([
            "profile:find-notes",
            {"space_id": "015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}
        ]));
        let notes: Vec<Value> = jedi::get(&["notes"], &noteval).unwrap();
        let note = jedi::stringify(&notes[0]).unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(note, r#"{"body":"AAYBAAzChZjAOGoAQ0MMjLofXXHarNfUu9Eqlv/063dUH4kbrp8Mnmw+XIn7LxAHloxdMpdiVDz5SAcLyy5DftjOjEEwKfylexz+C9zq5CQSjsQzuRQYMxD7TAwiJZLd+CsM1msek0kkhIB2whG6plMC8Hlyu1bMdcvWJ3B7Oonp89V57ycedVsSMWE28ablc3X3aKO8LRjCnoZlOK/UbZZYQnkm4roGV8dWlbKziTHm8R9ctBrxceo5ky3molooQ6GPKIPbm+lomsyrGDBG4DBDd7KlMJ1LCcsXzYWLnqvQyYny2ly37l5x3Y4dOcZVZ0gxkSzvHe37AzQl","has_file":false,"id":"015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e","keys":[{"k":"AAYBAAzuWB81LF46TLQ0b9aibwlL4lT5FTxw1UNxtUNKA2zuzW91drujc53uMQipFhcq6s6Ff9mDQr0Ew5H7Guw=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"mod":1497592545,"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","tags":["free market","america","fuck yeah"],"text":"COMMIES, AMIRITE?\n\n#PRIVATIZEEVERYTHING #FREEMARKET #TOLLROADS #LIBCUCKS","title":"YOU KNOW WUT I HATE?!","type":"text","user_id":51}"#);
        sleep(10);

        // i don't want a fucking email every time someone runs the tests
        //dispatch_ass(json!(["feedback:send", {"body": "I FORGOT MY PASSWORD CAN U PLEASE RESET IT?!?!?!"}]));
        dispatch_ass(json!(["sync:shutdown"]));
        dispatch_ass(json!(["user:logout"]));
        dispatch_ass(json!(["app:wipe-app-data"]));
        end(handle);
    }
}

