//! Thredder is a wrapper around a cpu thread pooling implementation. It works
//! using promises.

use std::marker::Send;
use futures::{Future, executor::ThreadPool};
use crate::error::TResult;

/// Stores state information for a thread we've spawned.
pub struct Thredder {
    /// Our Thredder's name
    pub name: String,
    /// Stores the thread pooler for this Thredder
    pool: ThreadPool,
}

impl Thredder {
    /// Create a new thredder
    pub fn new(name: &str, mut workers: u32) -> Thredder {
        if workers <= 0 {
            workers = 1;
        }
        let mut builder = ThreadPool::builder();
        builder.pool_size(workers as usize);
        Thredder {
            name: String::from(name),
            pool: builder.create().expect("Problem creating thread pool"),
        }
    }

    /// Run an operation on this pool, returning the Future to be waited on at
    /// a later time.
    pub fn run_async<F, T>(&self, run: F) -> impl Future<Output = TResult<T>>
        where T: Sync + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        let (tx, rx) = futures::channel::mpsc::unbounded::<String>();
        let op_fut = async move {
            let res = run();
            tx.unbounded_send(res).expect("Thredder::run_async() -- bad send");
        };
        self.pool.spawn_ok(op_fut);
        async {
            let res = rx.collect().await;
            res[0]
        }
    }

    /// Run an operation on this pool
    pub fn run<F, T>(&self, run: F) -> TResult<T>
        where T: Sync + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        self.pool.spawn_fn(run).wait()
    }
}

