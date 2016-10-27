//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::marker::Send;
use ::std::sync::Arc;
use ::std::ops::Deref;

use ::crossbeam::sync::MsQueue;
use ::futures::{self, Future, Canceled};
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TFutureResult, TError};
use ::util::thunk::Thunk;
use ::util::opdata::{OpData, OpConverter};
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

/// Stores state information for a thread we've spawned
pub struct Thredder {
    /// Our Thredder's name
    pub name: String,
    /// Allows sending messages to our thread
    tx: Pipeline,
    /// Stores the thread pooler for this Thredder
    pool: CpuPool,
}

impl Thredder {
    /// Create a new thredder
    pub fn new(name: &str, tx_main: Pipeline, workers: u32) -> Thredder {
        Thredder {
            name: String::from(name),
            tx: tx_main,
            pool: CpuPool::new(workers),
        }
    }

    /// Run an operation on this pool
    pub fn run<F, T>(&self, run: F) -> TFutureResult<T>
        where T: OpConverter + Send + 'static,
              F: FnOnce() -> TResult<T> + Send + 'static
    {
        let (fut_tx, fut_rx) = futures::oneshot::<TResult<OpData>>();
        let tx_main = self.tx.clone();
        let thread_name = String::from(&self.name[..]);
        self.pool.execute(|| run().map(|x| x.to_opdata()))
            .and_then(move |res: TResult<OpData>| {
                Ok(tx_main.next(move |_| { fut_tx.complete(res) }))
            }).forget();
        fut_rx
            .then(move |res: Result<TResult<OpData>, Canceled>| {
                match res {
                    Ok(x) => match x {
                        Ok(x) => futures::done(OpData::to_value(x)),
                        Err(x) => futures::done(Err(x)),
                    },
                    Err(_) => futures::done(Err(TError::Msg(format!("thredder: {}: pool oneshot future canceled", &thread_name)))),
                }
            })
            .boxed()
    }
}

