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
        let app_config = String::from(r#"{"data_folder":":memory:"}"#);
        turtl_core::start(app_config).join().unwrap();
    })
}

pub fn send(msg: &str) {
    let channel: String = config::get(&["messaging", "reqres"]).unwrap();
    carrier::send_string(&format!("{}-core-in", channel), String::from(&msg[..])).unwrap();
}

pub fn recv(mid: &str) -> String {
    let channel: String = config::get(&["messaging", "reqres"]).unwrap();
    String::from_utf8(carrier::recv(&format!("{}-core-out:{}", channel, mid)).unwrap()).unwrap()
}

pub fn event() -> String {
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
        if cmd == "quit" {
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
        send(&msg.as_str());
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        format!("here?");
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        let response = recv(req_str.as_str());
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        // TODO: why isn't this printing?????!?!?!
        format!("here?2");
        format!("response: {}", response);
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
        send(r#"["1","app:api:set-endpoint","https://api.turtl.it/v2"]"#);
        let msg = recv("1");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        end(handle);
    }

    #[test]
    fn login_sync_load_logout() {
        let handle = init();
        let username: String = config::get(&["client", "test", "username"]).unwrap();
        let password: String = config::get(&["client", "test", "password"]).unwrap();

        let msg = format!(r#"["2","user:login",{{"username":"{}","password":"{}"}}]"#, username, password);
        send(msg.as_str());
        let msg = recv("2");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["4","app:start-sync"]"#);
        send(msg.as_str());
        let msg = recv("4");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["6","profile:get-notes",{}]"#);
        send(msg.as_str());
        let msg = recv("6");
        assert_eq!(msg, r#"assss"#);
        sleep(10);

        let msg = String::from(r#"["6","app:shutdown-sync"]"#);
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

