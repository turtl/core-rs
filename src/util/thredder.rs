//! Thredder is a wrapper around a cpu thread pooling implementation. It works
//! using promises.

use ::std::marker::Send;

use ::futures::Future;
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TFutureResult};

/// Stores state information for a thread we've spawned.
pub struct Thredder {
    /// Our Thredder's name
    pub name: String,
    /// Stores the thread pooler for this Thredder
    pool: CpuPool,
}

impl Thredder {
    /// Create a new thredder
    pub fn new(name: &str, workers: u32) -> Thredder {
        Thredder {
            name: String::from(name),
            pool: CpuPool::new(workers as usize),
        }
    }

    /// Run an operation on this pool, returning the Future to be waited on at
    /// a later time.
    pub fn run_async<F, T>(&self, run: F) -> TFutureResult<T>
        where T: Sync + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        Box::new(self.pool.spawn_fn(run))
    }

    /// Run an operation on this pool
    pub fn run<F, T>(&self, run: F) -> TResult<T>
        where T: Sync + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        self.pool.spawn_fn(run).wait()
    }
}

