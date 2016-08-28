//! This is a very simple module that allows setting of a started/stopped state
//! for a loop from multiple places (thread-safe). It's useful for certain
//! modules that have loops in a thread that you might want to stop from other
//! threads.

use ::std::sync::RwLock;

pub struct Stopper {
    run: RwLock<bool>
}

impl Stopper {
    /// Create a new stopper
    pub fn new() -> Stopper {
        Stopper {
            run: RwLock::new(false),
        }
    }

    /// Sets the running state of the main thread
    pub fn set(&self, val: bool) {
        let mut guard = self.run.write().unwrap();
        *guard = val;
    }

    /// Check if we're running
    pub fn running(&self) -> bool {
        let guard = self.run.read();
        match guard {
            Ok(x) => *x,
            Err(_) => false,
        }
    }
}
