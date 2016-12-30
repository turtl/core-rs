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

        let msg = format!(r#"["69","app:wipe-local-data"]"#);
        send(msg.as_str());
        let msg = recv("69");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = format!(r#"["2","user:login","{}","{}"]"#, username, password);
        send(msg.as_str());
        let msg = recv("2");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = String::from(r#"["4","app:start-sync"]"#);
        send(msg.as_str());
        let msg = recv("4");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        // wait until we're loaded
        while recv_event() != r#"{"e":"profile:loaded","d":{}}"# {}

        let msg = String::from(r#"["12","profile:get-notes",["015874a823e4af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0361f"]]"#);
        send(msg.as_str());
        let msg = recv("12");
        assert_eq!(msg, r##"{"e":0,"d":[{"boards":["01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c"],"body":"AAUCAAGTaDVBJHRXgdsfHjrI4706aoh6HKbvoa6Oda4KP0HV07o4JEDED/QHqCVMTCODJq5o2I3DNv0jIhZ6U3686ViT6YIwi3EUFjnE+VMfPNdnNEMh7uZp84rUaKe03GBntBRNyiGikxn0mxG86CGnwBA8KPL1Gzwkxd+PJZhPiRz0enWbOBKik7kAztahJq7EFgCLdk7vKkhiTdOg4ghc/jD6s9ATeN8NKA90MNltzTIM","color":null,"embed":null,"file":null,"has_file":null,"id":"015874a823e4af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0361f","keys":null,"mod":1479425965,"password":null,"tags":[],"text":"the confederate flag is the flag of traitors","title":"mai title","type":"text","url":null,"user_id":"5244679b2b1375384f0000bc","username":null}]}"##);

        // wait until we're indexed
        while recv_event() != r#"{"e":"profile:indexed","d":{}}"# {}

        let msg = String::from(r#"["6","profile:find-notes",{"search":{"boards":["01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c"]}}]"#);
        send(msg.as_str());
        let msg = recv("6");
        assert_eq!(msg, r#"{"e":0,"d":[{"boards":["01549210bd2db6e84d965f99d2741739cf417b7df52f51008c55035365bc734b25fb2acbf5c9007c"],"body":"AAUCAAGTaDVBJHRXgdsfHjrI4706aoh6HKbvoa6Oda4KP0HV07o4JEDED/QHqCVMTCODJq5o2I3DNv0jIhZ6U3686ViT6YIwi3EUFjnE+VMfPNdnNEMh7uZp84rUaKe03GBntBRNyiGikxn0mxG86CGnwBA8KPL1Gzwkxd+PJZhPiRz0enWbOBKik7kAztahJq7EFgCLdk7vKkhiTdOg4ghc/jD6s9ATeN8NKA90MNltzTIM","color":null,"embed":null,"file":null,"has_file":null,"id":"015874a823e4af227c2eb2aca9cd869887e3f394033a7cd25f467f67dcf68a1a6699c3023ba0361f","keys":null,"mod":1479425965,"password":null,"tags":[],"text":"the confederate flag is the flag of traitors","title":"mai title","type":"text","url":null,"user_id":"5244679b2b1375384f0000bc","username":null}]}"#);
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

