//! Note: this are straight up copied from the main src/util/mod.rs file but I
//! really am too lazy to deal with yet another fucking crate so just deal,
//! everybody.

macro_rules! do_lock {
    ($lock:expr) => {{
        //println!(" >>> lock {} ({}::{})", stringify!($lock), file!(), line!());
        $lock.expect(concat!("turtl::util::do_lock!() -- failed to grab lock at ", file!(), "::", line!()))
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

