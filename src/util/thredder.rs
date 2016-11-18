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
        Thredder::op_converter(fut_rx, thread_name)
    }

    /// A static function to handle the conversion of a Future that has an
    /// OpData result.
    ///
    /// the reason behind this being separate instead of just lumped into
    /// Thredder.run() is that I did a bunch of work to split it out because I
    /// wanted to use this specific code in another part of Thredder, but ended
    /// up not going forward with that change. It took enough time to convert
    /// that I'd rather leave it split out for now.
    pub fn op_converter<T, O>(future: T, thread_name: String) -> TFutureResult<O>
        where T: Future<Item = TResult<OpData>, Error = Canceled> + Send + 'static,
              O: OpConverter + Send + 'static
    {
        future
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

