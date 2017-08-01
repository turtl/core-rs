extern crate carrier;
extern crate config;
extern crate turtl_core;
extern crate jedi;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;
use ::std::io::{self, Write};

use turtl_core::error::TResult;
use jedi::Value;

pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

pub fn init() -> thread::JoinHandle<()> {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }

    carrier::wipe();

    thread::spawn(|| {
        turtl_core::init().unwrap();
        let app_config = String::from(r#"{"data_folder":"/tmp/turtl/"}"#);
        turtl_core::start(app_config).join().unwrap();
    })
}

pub fn send(msg: &str) {
    let channel: String = config::get(&["messaging", "reqres"]).unwrap();
    carrier::send_string(&format!("{}-core-in", channel), String::from(&msg[..])).unwrap();
}

pub fn send_msg(msg: &str) -> TResult<()> {
    let channel: String = config::get(&["messaging", "reqres"])?;
    carrier::send_string(&format!("{}-core-in", channel), String::from(&msg[..]))?;
    Ok(())
}

pub fn recv(mid: &str) -> String {
    let channel: String = config::get(&["messaging", "reqres"]).unwrap();
    String::from_utf8(carrier::recv(&format!("{}-core-out:{}", channel, mid)).unwrap()).unwrap()
}

pub fn recv_msg(mid: &str) -> TResult<String> {
    let channel: String = config::get(&["messaging", "reqres"])?;
    Ok(String::from_utf8(carrier::recv(&format!("{}-core-out:{}", channel, mid))?)?)
}

pub fn recv_event() -> String {
    let channel: String = config::get(&["messaging", "events"]).unwrap();
    String::from_utf8(carrier::recv(channel.as_str()).unwrap()).unwrap()
}

fn repl() -> TResult<()> {
    let mut req_id = 1;
    loop {
        let req_str = format!("{}", req_id);
        io::stdout().write(&String::from("> ").as_bytes())?;
        io::stdout().flush()?;
        let mut cmd = String::new();
        io::stdin().read_line(&mut cmd)?;

        let mut parts: Vec<String> = cmd.as_str().split(" ")
            .filter(|x| x != &"")
            .map(|x| String::from(x.trim()))
            .collect::<Vec<_>>();
        if parts.len() == 0 { continue; }

        let cmd = parts.remove(0);

        // i GUESS i'll let you exit
        if cmd == "quit" || cmd == "q" {
            send(format!("[\"{}\",\"app:shutdown\"]", req_id).as_str());
            break;
        }

        let mut msg_parts: Vec<Value> = vec![Value::String(req_str.clone()), Value::String(cmd)];
        let mut args: Vec<Value> = parts.into_iter()
            .map(|x| {
                match jedi::parse::<Value>(&x) {
                    Ok(val) => val,
                    Err(_) => Value::String(format!("{}", x)),
                }
            })
            .collect::<Vec<_>>();
        msg_parts.append(&mut args);

        let msg = jedi::stringify(&msg_parts)?;
        send_msg(&msg.as_str())?;
        let response = recv_msg(req_str.as_str())?;
        println!("response: {}", response);
        req_id += 1;
    }
    Ok(())
}

fn main() {
    let handle = init();

    sleep(1000);
    println!("");
    println!("");
    println!("Welcome to the Turtl Client.");
    println!("");
    match repl() {
        Ok(_) => {},
        Err(err) => println!("turtl-client::repl() -- {}", err),
    }

    handle.join().unwrap();
}

#[cfg(test)]
mod tests {
    use ::std::thread;

    use ::carrier;
    use ::config;

    use super::*;

    fn end(handle: thread::JoinHandle<()>) {
        send(r#"["4269","app:shutdown"]"#);
        handle.join().unwrap();
        carrier::wipe();
    }

    #[test]
    fn ping_pong() {
        let handle = init();
        send(r#"["0","ping"]"#);
        let msg = recv("0");
        assert_eq!(msg, r#"{"e":0,"d":"pong"}"#);
        end(handle);
    }

    #[test]
    fn set_api_endpoint() {
        let handle = init();
        send(r#"["1","app:api:set-endpoint","http://api.turtl.dev:8181"]"#);
        let msg = recv("1");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        end(handle);
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

