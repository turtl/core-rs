//! This is the Carrier C API

use ::std::mem;
use ::std::ffi::CStr;
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
pub extern fn carrier_recv(channel_c: *const c_char, len_c: *mut usize) -> *const u8 {
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
        Ok(mut x) => {
            // make len == capacity
            x.shrink_to_fit();
            let ptr = x.as_mut_ptr();
            unsafe {
                *len_c = x.len();
                mem::forget(x);
            }
            ptr
        },
        Err(e) => {
            println!("carrier: recv: error: {}", e);
            unsafe { *len_c = 1; }
            return null;
        },
    }
}

#[no_mangle]
pub extern fn carrier_recv_nb(channel_c: *const c_char, len_c: *mut usize) -> *const u8 {
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
                Some(mut x) => {
                    // make len == capacity
                    x.shrink_to_fit();
                    let ptr = x.as_mut_ptr();
                    unsafe {
                        *len_c = x.len();
                        mem::forget(x);
                    }
                    ptr
                },
                None => return null,
            }
        },
        Err(e) => {
            println!("carrier: recv_nb: error: {}", e);
            unsafe { *len_c = 1; }
            return null;
        },
    }
}

#[no_mangle]
pub extern fn carrier_free(msg: *const u8, len: usize) -> i32 {
    let vec = unsafe { Vec::from_raw_parts(msg as *mut u8, len, len) };
    drop(vec);
    0
}

