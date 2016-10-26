extern crate carrier;
extern crate config;
extern crate turtl_core;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;

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
        let app_config = String::from(r#"{"data_folder":"d:/tmp/turtl/"}"#);
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

fn main() {
    let handle = init();

    send(r#"["0","ping"]"#);
    let msg = recv("0");
    println!("client: got {}", msg);
    send(r#"["1","app:shutdown"]"#);

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
        send(r#"["1","app:api:set_endpoint","https://api.turtl.it/v2"]"#);
        let msg = recv("1");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        end(handle);
    }

    #[test]
    fn login_logout() {
        let handle = init();
        let username: String = config::get(&["client", "test", "username"]).unwrap();
        let password: String = config::get(&["client", "test", "password"]).unwrap();

        let msg = format!(r#"["2","user:login",{{"username":"{}","password":"{}"}}]"#, username, password);
        send(msg.as_str());
        let msg = recv("2");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        let msg = event();
        assert_eq!(msg, r#"{"e":"sync:incoming:init","d":{}}"#);

        let msg = String::from(r#"["3","user:logout"]"#);
        send(msg.as_str());
        let msg = recv("3");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        sleep(10);

        send(r#"["3","app:shutdown"]"#);
        end(handle);
    }
}

