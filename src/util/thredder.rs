//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::sync::mpsc::Sender;
use ::std::marker::Send;

use ::futures::Future;
use ::futures_cpupool::CpuPool;

use ::error::TResult;
use ::util::json::Value;

#[derive(Debug)]
pub enum OpData {
    Bin(Vec<u8>),
    Str(String),
    JSON(Value),
}

/// A simple trait for allowing easy conversion from data into OpData
pub trait OpConvertible {
    /// Convert a piece of data into an OpData enum
    fn to_opdata(self) -> TResult<OpData>;
}
impl OpConvertible for TResult<Vec<u8>> {
    fn to_opdata(self) -> TResult<OpData> {
        self.map(|x| OpData::Bin(x))
    }
}
impl OpConvertible for TResult<String> {
    fn to_opdata(self) -> TResult<OpData> {
        self.map(|x| OpData::Str(x))
    }
}
impl OpConvertible for TResult<Value> {
    fn to_opdata(self) -> TResult<OpData> {
        self.map(|x| OpData::JSON(x))
    }
}

/// Stores state information for a thread we've spawned
pub struct Thredder {
    /// Our Thredder's name
    pub name: String,
    /// Allows sending messages to our thread
    tx: Sender<Box<Thunk>>,
    /// Stores the thread pooler for this Thredder
    pool: CpuPool,
}

/// Creates a way to call a Box<FnOnce> basically
pub trait Thunk: Send + 'static {
    fn call_box(self: Box<Self>);
}
impl<F: FnOnce() + Send + 'static> Thunk for F {
    fn call_box(self: Box<Self>) {
        (*self)();
    }
}

impl Thredder {
    /// Create a new thredder
    pub fn new(name: &str, tx_main: Sender<Box<Thunk>>, workers: u32) -> Thredder {
        Thredder {
            name: String::from(name),
            tx: tx_main,
            pool: CpuPool::new(workers),
        }
    }

    /// Run an operation on this pool
    pub fn run<F, C, T>(&self, run: F, handler: C)
        where T: OpConvertible + Send + 'static,
              F: FnOnce() -> T + Send + 'static,
              C: FnOnce(TResult<OpData>) + Send + 'static
    {
        let tx = self.tx.clone();
        let handler = Box::new(handler);
        self.pool.execute(|| {
            run().to_opdata()
        }).and_then(move |res: TResult<OpData>| {
            Ok(tx.send(Box::new(move || { handler(res); })))
        }).forget();
    }
}

