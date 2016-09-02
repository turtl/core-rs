use std::thread;
use std::time::Duration;

pub mod logger;
pub mod thunk;
pub mod opdata;
pub mod json;
pub mod event;
pub mod thredder;
pub mod stopper;
pub mod serialize;

pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

