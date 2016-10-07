#[macro_use]
extern crate lazy_static;
extern crate libsqlite3_sys as ffi;
#[macro_use]
extern crate quick_error;
extern crate rusqlite;

mod error;

use ::std::thread;
use ::std::time::Duration;
use ::std::sync::Mutex;
use ::std::ffi::{CStr, CString};
use ::std::ptr;
use ::std::os::raw::c_char;
use ::std::slice;
use ::std::mem::transmute;
use ::std::collections::HashMap;

use ::rusqlite::Connection;
use ::rusqlite::types::Value as SqlValue;
use ::rusqlite::Error as SqlError;

pub use ::error::CError;
use ::error::CResult;

lazy_static! {
    static ref CONN: Mutex<Carrier> = Mutex::new(Carrier::new().unwrap());
}

pub struct Carrier {
    db: Connection,
}

//unsafe impl Send for Carrier {}
//unsafe impl Sync for Carrier {}

impl Carrier {
    /// Create a new carrier
    pub fn new() -> CResult<Carrier> {
        //let flags = rusqlite::SQLITE_OPEN_READ_WRITE | rusqlite::SQLITE_OPEN_CREATE | rusqlite::SQLITE_OPEN_FULL_MUTEX | rusqlite::SQLITE_OPEN_URI;
        //let conn = try!(Connection::open_in_memory_with_flags(flags));
        let conn = try!(Connection::open_in_memory());
        try!(conn.execute("CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY, channel VARCHAR(32), message blob)", &[]));
        try!(conn.execute("CREATE INDEX IF NOT EXISTS idx_message_channel ON messages (channel, id)", &[]));
        Ok(Carrier {
            db: conn,
        })
    }
}

/// Send a message on a channel
pub fn send(channel: &str, message: &Vec<u8>) -> CResult<()> {
    let ref conn = (*CONN).lock().unwrap().db;
    conn.execute("INSERT INTO messages (channel, message) VALUES ($1, $2)", &[&channel, message])
        .map(|_| ())
        .map_err(|e| From::from(e))
}

/// Send a message on a channel
pub fn send_string(channel: &str, message: &String) -> CResult<()> {
    let vec = Vec::from(message.as_bytes());
    send(channel, &vec)
}

/// Blocking receive
pub fn recv(channel: &str) -> CResult<Vec<u8>> {
    let delay_ms = 10;
    loop {
        let msg = try!(recv_nb(channel));
        match msg {
            Some(x) => return Ok(x),
            _ => (),
        }
        thread::sleep(Duration::from_millis(delay_ms));
    }
}

/// Non-blocking receive
pub fn recv_nb(channel: &str) -> CResult<Option<Vec<u8>>> {
    let ref conn = (*CONN).lock().unwrap().db;

    let query = "SELECT id, message FROM messages WHERE channel = $1 ORDER BY id ASC LIMIT 1";
    let res = conn.query_row_and_then(query, &[&channel], |row| -> CResult<(i64, Vec<u8>)> {
        let id: i64 = match try!(row.get_checked("id")) {
            SqlValue::Integer(x) => x,
            _ => return Err(CError::Msg(format!("carrier: recv_nb: `id` field is not an i64"))),
        };
        let data: SqlValue = try!(row.get_checked("message"));
        match data {
            SqlValue::Blob(x) => {
                Ok((id, x))
            },
            _ => return Err(CError::Msg(format!("carrier: recv_nb: `message` field is not a blob"))),
        }
    });
    match res {
        Ok(x) => {
            let (id, data) = x;
            try!(conn.execute("DELETE FROM messages WHERE id = $1", &[&id]));
            Ok(Some(data))
        }
        Err(e) => match e {
            CError::SqlError(e) => match e {
                SqlError::QueryReturnedNoRows => Ok(None),
                _ => Err(From::from(e)),
            },
            _ => Err(e),
        },
    }
}

/// Clean up DB memory
pub fn vacuum() -> CResult<()> {
    let ref conn = (*CONN).lock().unwrap().db;
    conn.execute("VACUUM", &[])
        .map(|_| ())
        .map_err(|e| From::from(e))
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
    let res = match send(channel, &message) {
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
pub extern fn carrier_vacuum() -> i32 {
    match vacuum() {
        Ok(_) => 0,
        Err(e) => {
            println!("carrier: vacuum: error: {}", e);
            -1
        }
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
    use ::std::time::Duration;

    use super::*;

    fn sleep(millis: u64) {
        thread::sleep(Duration::from_millis(millis));
    }

    #[test]
    fn send_recv_simple() {
        send("messages", &Vec::from(String::from("this is a test").as_bytes())).unwrap();
        send_string("messages", &String::from("this is another test")).unwrap();

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
            sleep(200);
            send_string("core", &String::from("hello, there")).unwrap();
        });
        let msg = String::from_utf8(recv("core", 10).unwrap()).unwrap();
        assert_eq!(msg, "hello, there");
        handle.join().unwrap();
    }
}
