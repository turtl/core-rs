//! This is the Carrier C API

use ::std::mem::transmute;
use ::std::ffi::{CStr, CString};
use ::std::ptr;
use ::std::os::raw::c_char;
use ::std::slice;

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
    let res = match ::send(channel, message) {
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
    match ::recv(channel) {
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
    match ::recv_nb(channel) {
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


