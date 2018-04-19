//! A simple crate the wraps the turtl core shared lib and adds some handy dandy
//! functions around it so we can call into it without having to wrap the C API
//! each fing time. Great for integration tests or a websocket wrapper etc etc.
use ::std::{env, thread, slice, str};
use ::std::ffi::CString;

// -----------------------------------------------------------------------------
// Turtl C wrapper
// -----------------------------------------------------------------------------
// i made this
extern "C" {
    pub fn turtlc_start(config: *const ::std::os::raw::c_char, threaded: u8) -> i32;
    pub fn turtlc_send(message_bytes: *const u8, message_len: usize) -> i32;
    pub fn turtlc_recv(non_block: u8, msgid: *const ::std::os::raw::c_char, len: *mut usize) -> *const u8;
    pub fn turtlc_recv_event(non_block: u8, len: *mut usize) -> *const u8;
    pub fn turtlc_free(msg: *const u8, len: usize) -> i32;
}

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------
/// Init turtl
pub fn init(app_config: &str) -> thread::JoinHandle<()> {
    if env::var("TURTL_CONFIG_FILE").is_err() {
        env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
    }

    let config_copy = String::from(app_config);
    let handle = thread::spawn(move || {
        // send in a the config options we need for our tests
        let app_config = config_copy.as_str();
        let app_config_c = CString::new(app_config).unwrap();
        let ret = unsafe {
            turtlc_start(app_config_c.as_ptr(), 0)
        };
        if ret != 0 {
            panic!("Error running turtl: err {}", ret);
        }
    });
    handle
}

/// Send a message to the core
pub fn send(msg: &str) {
    let msg_vec = Vec::from(String::from(msg).as_bytes());
    let ret = unsafe {
        turtlc_send(msg_vec.as_ptr(), msg_vec.len())
    };
    if ret != 0 {
        panic!("Error sending msg: err {}", ret);
    }
}

/// Receive a message from the core, blocking (note that this *requires*
/// {"reqres_append_mid": true} in the app config!)
pub fn recv(mid: &str) -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let mid_c = CString::new(mid).unwrap();
    let msg_c = unsafe {
        turtlc_recv(0, mid_c.as_ptr(), raw_len)
    };
    assert!(!msg_c.is_null());
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).unwrap();
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
}

/// Receive a core event (blocks)
pub fn recv_event() -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let msg_c = unsafe {
        turtlc_recv_event(0, raw_len)
    };
    assert!(!msg_c.is_null());
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).unwrap();
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
}

