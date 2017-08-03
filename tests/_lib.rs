extern crate config;
extern crate carrier;
extern crate turtl_core;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;
use turtl_core::error::TResult;

#[allow(dead_code)]
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

pub fn init() -> thread::JoinHandle<()> {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "config.yaml");
    }

    carrier::wipe();

    thread::spawn(|| {
        turtl_core::init().unwrap();
        // this is more or less ignored when testing, we use in-memory dbs and
        // config.integration_tests.data_folder for file storage
        let app_config = String::from(r#"{"data_folder":":memory:"}"#);
        let handle = turtl_core::start(app_config);
        sleep(100);
        handle.join().unwrap();
    })
}

pub fn end(handle: thread::JoinHandle<()>) {
    send(r#"["4269","app:shutdown"]"#);
    handle.join().unwrap();
    carrier::wipe();
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

