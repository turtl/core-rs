//! Thredder is a wrapper around a cpu thread pooling implementation. It works
//! using promises.

use std::marker::Send;
use log::{error};
use futures::{executor::ThreadPool, StreamExt};

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

    pub fn pool(&self) -> &ThreadPool {
        &self.pool
    }

    /// Run an operation on this pool
    pub fn run<F, T>(&self, run: F) -> T
        where T: Sync + Send + 'static,
              F: FnOnce() -> T + Send + 'static
    {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<T>();
        let op_fut = async move {
            let res: T = run();
            match tx.unbounded_send(res) {
                Ok(_) => {}
                Err(e) => error!("Thredder.run() -- error sending result back to caller: {:?}", e),
            };
        };
        self.pool.spawn_ok(op_fut);
        let res_future = async move {
            let res: Option<T> = rx.next().await;
            res.unwrap()
        };
        let res: T = futures::executor::block_on(res_future);
        res
    }
}

