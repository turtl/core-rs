//! A simple crate the wraps the turtl core shared lib and adds some handy dandy
//! functions around it so we can call into it without having to wrap the C API
//! each fing time. Great for integration tests or a websocket wrapper etc etc.
use ::std::{env, thread, slice, str};
use ::std::ffi::CString;
use ::std::time::Duration;

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
    pub fn turtlc_lasterr() -> *mut ::std::os::raw::c_char;
    pub fn turtlc_free_err(lasterr: *mut ::std::os::raw::c_char) -> i32;
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
        let app_config_c = CString::new(app_config).expect("cwrap::init() -- failed to convert config to CString");
        let ret = unsafe {
            turtlc_start(app_config_c.as_ptr(), 0)
        };
        if ret != 0 {
            panic!("Error running turtl: err {}", ret);
        }
    });
    // since we're starting our own thread here, we need to wait for the stinkin
    // core to load its config before we start asking it for stuff.
    thread::sleep(Duration::from_millis(500));
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

/// Receive a message from the core, blocking (note that if you are not using
/// {"reqres_append_mid": true} in the app config, you should pass "" for the
/// `mid` arg here).
pub fn recv(mid: &str) -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let mid_c = CString::new(mid).expect("cwrap::recv() -- failed to convert mid to CString");
    let msg_c = unsafe {
        turtlc_recv(0, mid_c.as_ptr(), raw_len)
    };
    if msg_c.is_null() && len > 0 {
        match lasterr() {
            Some(x) => panic!("recv() -- error getting event: {}", x),
            None => panic!("recv() -- got empty msg and couldn't grab lasterr"),
        }
    }
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).expect("cwrap::recv() -- failed to parse utf8 str");
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
}

/// Like recv, but non-blocking
pub fn recv_nb(mid: &str) -> Option<String> {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let mid_c = CString::new(mid).expect("cwrap::recv_nb() -- failed to convert mid to CString");
    let msg_c = unsafe {
        turtlc_recv(1, mid_c.as_ptr(), raw_len)
    };
    if msg_c.is_null() {
        return None;
    }
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).expect("cwrap::recv_nb() -- failed to parse utf8 str");
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    Some(ret)
}

/// Receive a core event (blocks)
pub fn recv_event() -> String {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let msg_c = unsafe {
        turtlc_recv_event(0, raw_len)
    };
    if msg_c.is_null() && len > 0 {
        match lasterr() {
            Some(x) => panic!("recv_event() -- error getting event: {}", x),
            None => panic!("recv_event() -- got empty msg and couldn't grab lasterr"),
        }
    }
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).expect("cwrap::recv_event() -- failed to parse utf8 str");
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    ret
}

/// Receive a core event (non blocking)
pub fn recv_event_nb() -> Option<String> {
    let mut len: usize = 0;
    let raw_len = &mut len as *mut usize;
    let msg_c = unsafe {
        turtlc_recv_event(1, raw_len)
    };
    if msg_c.is_null() {
        return None;
    }
    let slice = unsafe { slice::from_raw_parts(msg_c, len) };
    let res_str = str::from_utf8(slice).expect("cwrap::recv_event_nb() -- failed to parse utf8 str");
    let ret = String::from(res_str);
    unsafe {
        turtlc_free(msg_c, len);
    }
    Some(ret)
}

pub fn lasterr() -> Option<String> {
    let ptr = unsafe { turtlc_lasterr() };
    if ptr.is_null() {
        return None;
    }
    let cstring = unsafe { CString::from_raw(ptr) };
    match cstring.into_string() {
        Ok(x) => Some(x),
        Err(e) => {
            println!("lasterr() -- error grabbing last error: {}", e);
            None
        }
    }
}

