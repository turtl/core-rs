//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::marker::Send;
use ::std::sync::Arc;
use ::std::ops::Deref;

use ::crossbeam::sync::MsQueue;
use ::futures::{self, Future};
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TFutureResult, TError};
use ::util::thunk::Thunk;
use ::turtl::TurtlWrap;

/// Abstract our tx_main type
pub struct Pipeline {
    tx: Arc<MsQueue<Box<Thunk<TurtlWrap>>>>,
}

impl Pipeline {
    /// Create a new Pipeline
    pub fn new() -> Pipeline {
        Pipeline {
            tx: Arc::new(MsQueue::new()),
        }
    }

    /// Create a new pipeline from a tx object
    pub fn new_tx(tx: Arc<MsQueue<Box<Thunk<TurtlWrap>>>>) -> Pipeline {
        // bangarang
        Pipeline {
            tx: tx,
        }
    }

    /// Run the given function on the next main loop
    pub fn next<F>(&self, cb: F)
        where F: FnOnce(TurtlWrap) + Send + Sync + 'static
    {
        self.tx.push(Box::new(cb));
    }

    /// Return a future that resolves with a TurtlWrap object.
    pub fn next_fut(&self) -> TFutureResult<TurtlWrap> {
        let (fut_tx, fut_rx) = futures::oneshot::<TurtlWrap>();
        self.next(move |turtl| { fut_tx.complete(turtl); });
        fut_rx
            .map_err(|_| TError::Msg(String::from("Pipeline::next_fut() -- future canceled")))
            .boxed()
    }
}
impl Deref for Pipeline {
    type Target = Arc<MsQueue<Box<Thunk<TurtlWrap>>>>;

    fn deref(&self) -> &Arc<MsQueue<Box<Thunk<TurtlWrap>>>> {
        &self.tx
    }
}
impl Clone for Pipeline {
    fn clone(&self) -> Self {
        Pipeline::new_tx(self.tx.clone())
    }
}

/// Stores state information for a thread we've spawned.
///
/// NOTE: Thredder used to have a lot of wrapping around CpuPool and provided a
/// lot of utilities for passing data between pools and our main thread. Those
/// days are gone now since many improvements to CpuPool, so it now exists as a
/// very thin layer.
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

    /// Run an operation on this pool
    pub fn run<F, T>(&self, run: F) -> TFutureResult<T>
        where T: Sync + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        self.pool.spawn_fn(run).boxed()
    }
}

