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
    let channel: String = config::get(&["messaging", "address"]).unwrap();
    carrier::send_string(&format!("{}-core-in", channel), String::from(&msg[..])).unwrap();
}

pub fn recv() -> String {
    let channel: String = config::get(&["messaging", "address"]).unwrap();
    String::from_utf8(carrier::recv(&format!("{}-core-out", channel)).unwrap()).unwrap()
}

fn main() {
    let handle = init();

    send(r#"["ping"]"#);
    let msg = recv();
    println!("client: got {}", msg);

    handle.join().unwrap();
}

#[cfg(test)]
mod tests {
    use ::std::thread;

    use ::carrier;
    use ::config;

    use super::*;

    fn end(handle: thread::JoinHandle<()>) {
        handle.join().unwrap();
        carrier::wipe();
    }

    #[test]
    fn inits_shuts_down() {
        let handle = init();
        send(r#"["ping"]"#);
        let msg = recv();
        assert_eq!(msg, r#"{"e":"pong"}"#);
        sleep(10);
        send(r#"["app:shutdown"]"#);
        let msg = recv();
        assert_eq!(msg, r#"{"e":"shutdown"}"#);
        end(handle);
    }

    #[test]
    fn login() {
        let handle = init();
        let username: String = config::get(&["client", "test", "username"]).unwrap();
        let password: String = config::get(&["client", "test", "password"]).unwrap();
        let msg = format!(r#"["user:login",{{"username":"{}","password":"{}"}}]"#, username, password);
        send(msg.as_str());
        let msg = recv();
        assert_eq!(msg, r#"{"e":"login-success"}"#);
        sleep(10);
        send(r#"["app:shutdown"]"#);
        end(handle);
    }
}

