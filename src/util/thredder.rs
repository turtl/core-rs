//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::marker::Send;
use ::std::sync::Arc;

use ::crossbeam::sync::MsQueue;
use ::futures::{self, Future, Canceled};
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TFutureResult, TError};
use ::util::thunk::Thunk;
use ::util::opdata::{OpData, OpConverter};
use ::turtl::{TurtlWrap};

/// Abstract our tx_main type
pub type Pipeline = Arc<MsQueue<Box<Thunk<TurtlWrap>>>>;

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
                Ok(tx_main.push(Box::new(move |_: TurtlWrap| { fut_tx.complete(res) })))
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

