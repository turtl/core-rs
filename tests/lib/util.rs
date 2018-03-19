extern crate config;
extern crate carrier;
extern crate turtl_core;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use ::std::env;
use ::std::thread;
use ::std::time::Duration;
use turtl_core::error::TResult;
use ::jedi::Value;
use ::std::sync::RwLock;

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    e: u32,
    d: Value,
}

lazy_static! {
    /// create a static/global CONFIG var, and load it with our config data
    static ref MID: RwLock<u64> = RwLock::new(0);
}

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
        // send in a the config options we need for our tests
        let app_config = String::from(r#"{
            "data_folder": ":memory:",
            "integration_tests": {"incoming_sync_timeout": 5},
            "wrap_errors": true,
            "messaging": {"reqres_append_mid": true},
            "sync": {
                "enable_incoming": true,
                "enable_outgoing": true,
                "enable_files_incoming": true,
                "enable_files_outgoing": true
            }
        }"#);
        let handle = turtl_core::start(app_config);
        sleep(100);
        handle.join().unwrap();
    })
}

pub fn end(handle: thread::JoinHandle<()>) {
    dispatch(json!(["app:shutdown"]));
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

pub fn dispatch(args: Value) -> Response {
    let msg_id = {
        let mut mid_guard = MID.write().unwrap();
        let mid = *mid_guard;
        *mid_guard += 1;
        mid.to_string()
    };
    let mut msg_args = vec![jedi::to_val(&msg_id).unwrap()];
    let mut vals = jedi::from_val::<Vec<Value>>(args).unwrap();
    msg_args.append(&mut vals);
    let msg = jedi::stringify(&msg_args).unwrap();
    send(msg.as_str());
    let recv = recv(msg_id.as_str());
    jedi::parse(&recv).unwrap()
}

pub fn dispatch_ass(args: Value) -> Value {
    let res = dispatch(args);
    if res.e != 0 {
        panic!("dispatch: {}", res.d);
    }
    let Response {e: _e, d} = res;
    d
}

pub fn recv_msg(mid: &str) -> TResult<String> {
    let channel: String = config::get(&["messaging", "reqres"])?;
    Ok(String::from_utf8(carrier::recv(&format!("{}-core-out:{}", channel, mid))?)?)
}

pub fn recv_event() -> String {
    let channel: String = config::get(&["messaging", "events"]).unwrap();
    String::from_utf8(carrier::recv(channel.as_str()).unwrap()).unwrap()
}

pub fn wait_on(evname: &str) -> Value {
    loop {
        let ev = recv_event();
        let parsed: Value = jedi::parse(&ev).unwrap();
        let parsed_evname: String = jedi::get(&["e"], &parsed).unwrap();
        if parsed_evname == evname {
            return jedi::get(&["d"], &parsed).unwrap();
        }
    }
}

