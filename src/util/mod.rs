use std::thread;
use std::time::Duration;

pub mod logger;
#[macro_use]
pub mod serialization;
pub mod json;

pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

