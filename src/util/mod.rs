use ::std::thread;
use ::std::time::Duration;
use ::error::TResult;
use ::std::io;
use ::std::fs;
use ::std::path::Path;

macro_rules! do_lock {
    ($lock:expr) => {{
        $lock.unwrap()
    }}
}

/// A macro that wraps locking mutexes. Really handy for debugging deadlocks.
#[macro_export]
macro_rules! lock {
    ($lockable:expr) => { do_lock!($lockable.lock()) }
}

/// A macro that wraps read-locking RwLocks. Really handy for debugging
/// deadlocks.
#[macro_export]
macro_rules! lockr {
    ($lockable:expr) => { do_lock!($lockable.read()) }
}

/// A macro that wraps write-locking RwLocks. Really handy for debugging
/// deadlocks.
#[macro_export]
macro_rules! lockw {
    ($lockable:expr) => { do_lock!($lockable.write()) }
}

pub mod logger;
pub mod event;
pub mod thredder;
#[macro_use]
pub mod ser;

/// Go to sleeeeep
pub fn sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}

/// Create a directory if it doesn't exist
pub fn create_dir<P: AsRef<Path>>(dir: P) -> TResult<()> {
    // std::fs, for me please, we're lookin at china. we're lookin at the UN. go
    // ahead and create our directory.
    match fs::create_dir_all(dir) {
        Ok(_) => {
            Ok(())
        },
        Err(e) => {
            match e.kind() {
                // talked to drew about directory already existing. sounds good.
                io::ErrorKind::AlreadyExists => Ok(()),
                _ => return Err(From::from(e)),
            }
        }
    }
}

