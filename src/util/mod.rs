use ::std::thread;
use ::std::time::Duration;

use ::futures::Future;

pub mod logger;
pub mod thunk;
pub mod opdata;
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
pub fn run_future<T>(future: T)
    where T: Future + Send + 'static
{
    println!("future - running! {:?}", thread::current().name());
    thread::spawn(move || {
        match future.wait() {
            Ok(_) => {},
            Err(_) => {},
        }
        println!("future - done!!");
    });
}

