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
    fn login_sync_load_logout() {
        let handle = init();
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        let msg = format!(r#"["69","app:wipe-app-data"]"#);
        send(msg.as_str());
        let msg = recv("69");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = format!(r#"["2","user:login","{}","{}"]"#, username, password);
        send(msg.as_str());
        let msg = recv("2");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["4","sync:start"]"#);
        send(msg.as_str());
        let msg = recv("4");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        // wait until we're loaded
        while recv_event() != r#"{"e":"profile:loaded","d":{}}"# {}
        // wait until we're indexed
        while recv_event() != r#"{"e":"profile:indexed","d":{}}"# {}

        // wait for sync to complete
        sleep(1000);

        let msg = format!(r#"["30","profile:load"]"#);
        send(msg.as_str());
        let msg = recv("30");
        let val: Value = jedi::parse(&msg).unwrap();
        let data: Value = jedi::get(&["d"], &val).unwrap();
        let spaces: Vec<Value> = jedi::get(&["spaces"], &data).unwrap();
        let boards: Vec<Value> = jedi::get(&["boards"], &data).unwrap();
        let ptitle: String = jedi::get(&["spaces", "0", "title"], &data).unwrap();
        assert_eq!(spaces.len(), 3);
        assert_eq!(boards.len(), 3);
        assert_eq!(ptitle, "Personal");
        sleep(10);

        let msg = String::from(r#"["6","profile:find-notes",{"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}]"#);
        send(msg.as_str());
        let msg = recv("6");
        let res: Value = jedi::parse(&msg).unwrap();
        let notes: Vec<Value> = jedi::get(&["d"], &res).unwrap();
        let note = jedi::stringify(&notes[0]).unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(note, r#"{"body":"AAYBAAzChZjAOGoAQ0MMjLofXXHarNfUu9Eqlv/063dUH4kbrp8Mnmw+XIn7LxAHloxdMpdiVDz5SAcLyy5DftjOjEEwKfylexz+C9zq5CQSjsQzuRQYMxD7TAwiJZLd+CsM1msek0kkhIB2whG6plMC8Hlyu1bMdcvWJ3B7Oonp89V57ycedVsSMWE28ablc3X3aKO8LRjCnoZlOK/UbZZYQnkm4roGV8dWlbKziTHm8R9ctBrxceo5ky3molooQ6GPKIPbm+lomsyrGDBG4DBDd7KlMJ1LCcsXzYWLnqvQyYny2ly37l5x3Y4dOcZVZ0gxkSzvHe37AzQl","has_file":false,"id":"015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e","keys":[{"k":"AAYBAAzuWB81LF46TLQ0b9aibwlL4lT5FTxw1UNxtUNKA2zuzW91drujc53uMQipFhcq6s6Ff9mDQr0Ew5H7Guw=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"mod":1497592545,"space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","tags":["free market","america","fuck yeah"],"text":"COMMIES, AMIRITE?\n\n#PRIVATIZEEVERYTHING #FREEMARKET #TOLLROADS #LIBCUCKS","title":"YOU KNOW WUT I HATE?!","type":"text","user_id":51}"#);
        sleep(10);

        // i don't want a fucking email every time someone runs the tests
        //let msg = String::from(r#"["42","feedback:send",{"body":"I FORGOT MY PASSWORD CAN U PLEASE RESET IT?!?!?!"}]"#);
        //send(msg.as_str());
        //let msg = recv("42");
        //assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        //sleep(10);

        let msg = String::from(r#"["6","sync:shutdown"]"#);
        send(msg.as_str());
        let msg = recv("6");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["3","user:logout"]"#);
        send(msg.as_str());
        let msg = recv("3");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        send(r#"["3","app:shutdown"]"#);
        end(handle);
    }
}

