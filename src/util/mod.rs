use ::std::thread;
use ::std::time::Duration;

use ::futures::Future;

pub mod logger;
pub mod thunk;
pub mod event;
pub mod thredder;
pub mod stopper;
#[macro_use]
pub mod serialize;

/// Go to sleeeeep
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

/// Drive a future forward
/// TODO: run on calling thread somehow (main)
pub fn run_future<T>(future: T)
    where T: Future + Send + 'static
{
    thread::Builder::new().name(String::from("future-runner")).spawn(move || {
        match future.wait() {
            Ok(_) => {},
            Err(_) => {},
        }
    }).unwrap();
}

