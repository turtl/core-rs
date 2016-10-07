extern crate crossbeam;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate quick_error;

mod error;

use ::std::sync::{Arc, RwLock};
use ::std::ffi::{CStr, CString};
use ::std::ptr;
use ::std::os::raw::c_char;
use ::std::slice;
use ::std::mem::transmute;
use ::std::collections::HashMap;

use ::crossbeam::sync::MsQueue;

pub use ::error::CError;
use ::error::CResult;

lazy_static! {
    static ref CONN: Carrier = Carrier::new().unwrap();
}

pub struct Carrier {
    queues: RwLock<HashMap<String, Arc<MsQueue<Vec<u8>>>>>,
}

//unsafe impl Send for Carrier {}
//unsafe impl Sync for Carrier {}

impl Carrier {
    /// Create a new carrier
    pub fn new() -> CResult<Carrier> {
        Ok(Carrier {
            queues: RwLock::new(HashMap::new()),
        })
    }

    /// Ensure a channel exists
    pub fn ensure(&self, channel: &String) -> Arc<MsQueue<Vec<u8>>> {
        let mut guard = self.queues.write().unwrap();
        if (*guard).contains_key(channel) {
            (*guard).get(channel).unwrap().clone()
        } else {
            let queue = Arc::new(MsQueue::new());
            (*guard).insert(channel.clone(), queue.clone());
            queue
        }
    }
}

/// Send a message on a channel
pub fn send(channel: &str, message: Vec<u8>) -> CResult<()> {
    let queue = (*CONN).ensure(&String::from(channel));
    queue.push(message);
    Ok(())
}

/// Send a message on a channel
pub fn send_string(channel: &str, message: String) -> CResult<()> {
    let vec = Vec::from(message.as_bytes());
    send(channel, vec)
}

/// Blocking receive
pub fn recv(channel: &str) -> CResult<Vec<u8>> {
    let queue = (*CONN).ensure(&String::from(channel));
    Ok(queue.pop())
}

/// Non-blocking receive
pub fn recv_nb(channel: &str) -> CResult<Option<Vec<u8>>> {
    let queue = (*CONN).ensure(&String::from(channel));
    Ok(queue.try_pop())
}

// -----------------------------------------------------------------------------
// our C api
// -----------------------------------------------------------------------------
#[no_mangle]
pub extern fn carrier_send(channel_c: *const c_char, message_bytes: *const u8, message_len: usize) -> i32 {
    if channel_c.is_null() { return -1; }
    if message_bytes.is_null() { return -1; }
    let channel_res = unsafe { CStr::from_ptr(channel_c).to_str() };
    let channel = match channel_res {
        Ok(x) => x,
        Err(e) => {
            println!("carrier: send: error: {}", e);
            return -3;
        },
    };
    let message = Vec::from(unsafe { slice::from_raw_parts(message_bytes, message_len) });
    let res = match send(channel, message) {
        Ok(_) => 0,
        Err(e) => {
            println!("carrier: send: error: {}", e);
            return -4;
        },
    };
    res
}

#[no_mangle]
pub extern fn carrier_recv(channel_c: *const c_char, len_c: *mut u64) -> *mut u8 {
    let null = ptr::null_mut();
    unsafe { *len_c = 0; }
    if channel_c.is_null() { return null; }
    let channel_res = unsafe { CStr::from_ptr(channel_c).to_str() };
    let channel = match channel_res {
        Ok(x) => x,
        Err(e) => {
            println!("carrier: recv: error: {}", e);
            return null;
        },
    };
    match recv(channel) {
        Ok(x) => {
            unsafe { *len_c = x.len() as u64; }
            match CString::new(x) {
                Ok(x) => unsafe { transmute(Box::new(x.into_raw())) },
                Err(e) => {
                    println!("carrier: recv: error: {}", e);
                    return null;
                }
            }
        },
        Err(e) => {
            println!("carrier: recv: error: {}", e);
            return null;
        },
    }
}

#[no_mangle]
pub extern fn carrier_recv_nb(channel_c: *const c_char, len_c: *mut u64) -> *mut u8 {
    let null = ptr::null_mut();
    unsafe { *len_c = 0; }
    if channel_c.is_null() { return null; }
    let channel_res = unsafe { CStr::from_ptr(channel_c).to_str() };
    let channel = match channel_res {
        Ok(x) => x,
        Err(e) => {
            println!("carrier: recv_nb: error: {}", e);
            return null;
        },
    };
    match recv_nb(channel) {
        Ok(x) => {
            match x {
                Some(x) => {
                    unsafe { *len_c = x.len() as u64; }
                    match CString::new(x) {
                        Ok(x) => unsafe { transmute(Box::new(x.into_raw())) },
                        Err(e) => {
                            println!("carrier: recv_nb: error: {}", e);
                            return null;
                        }
                    }
                },
                None => return null,
            }
        },
        Err(e) => {
            println!("carrier: recv_nb: error: {}", e);
            return null;
        },
    }
}

#[no_mangle]
pub extern fn carrier_free(msg: *mut u8) -> i32 {
    let rmsg: Box<Vec<u8>> = unsafe { transmute(msg) };
    drop(rmsg);
    0
}

#[cfg(test)]
mod tests {
    use ::std::thread;

    use super::*;

    #[test]
    fn send_recv_simple() {
        send("messages", Vec::from(String::from("this is a test").as_bytes())).unwrap();
        send_string("messages", String::from("this is another test")).unwrap();

        let next = String::from_utf8(recv_nb("messages").unwrap().unwrap()).unwrap();
        assert_eq!(next, "this is a test");
        let next = String::from_utf8(recv_nb("messages").unwrap().unwrap()).unwrap();
        assert_eq!(next, "this is another test");
        let next = recv_nb("messages").unwrap();
        assert_eq!(next, None);
        let next = recv_nb("messages").unwrap();
        assert_eq!(next, None);
        let next = recv_nb("nope").unwrap();
        assert_eq!(next, None);
    }

    #[test]
    fn recv_blocking() {
        let handle = thread::spawn(move || {
            send_string("core", String::from("hello, there")).unwrap();
        });
        let msg = String::from_utf8(recv("core").unwrap()).unwrap();
        assert_eq!(msg, "hello, there");
        handle.join().unwrap();
    }
}
