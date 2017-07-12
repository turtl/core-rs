use ::std::thread;
use ::std::time::Duration;

pub mod logger;
pub mod thunk;
pub mod event;
pub mod thredder;
#[macro_use]
pub mod macros;
pub mod ser;

/// Go to sleeeeep
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

