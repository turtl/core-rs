//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::sync::mpsc::Sender;
use ::std::marker::Send;

use ::futures::{self, Future, Canceled};
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TFutureResult, TError};
use ::util::json::Value;
use ::turtl::{TurtlWrap};

#[derive(Debug)]
pub enum OpData {
    Bin(Vec<u8>),
    Str(String),
    JSON(Value),
    Null(()),
}

/// A simple trait for allowing easy conversion from data into OpData
pub trait OpConverter : Sized {
    /// Convert a piece of data into an OpData enum
    fn to_opdata(self) -> OpData;

    /// Convert an OpData back to its raw form
    fn to_value(OpData) -> TResult<Self>;
}

impl OpData {
    /// Convert an OpData into its raw contained self
    pub fn to_value<T>(val: OpData) -> TResult<T>
        where T: OpConverter
    {
        T::to_value(val)
    }
}

/// Makes creating conversions between Type -> OpData and back again easy
macro_rules! make_converter {
    ($conv_type:ty, $enumfield:ident) => (
        impl OpConverter for $conv_type {
            fn to_opdata(self) -> OpData {
                OpData::$enumfield(self)
            }

            fn to_value(data: OpData) -> TResult<Self> {
                match data {
                    OpData::$enumfield(x) => Ok(x),
                    _ => Err(TError::BadValue(format!("OpConverter: problem converting {}", stringify!($conv_type)))),
                }
            }
        }
    )
}

make_converter!(Vec<u8>, Bin);
make_converter!(String, Str);
make_converter!(Value, JSON);
make_converter!((), Null);

/// Creates a way to call a Box<FnOnce> basically
pub trait Thunk: Send + 'static {
    fn call_box(self: Box<Self>, TurtlWrap);
}
impl<F: FnOnce(TurtlWrap) + Send + 'static> Thunk for F {
    fn call_box(self: Box<Self>, turtl: TurtlWrap) {
        (*self)(turtl);
    }
}

/// Abstract our tx_main type
pub type Pipeline = Sender<Box<Thunk>>;

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
              F: FnOnce() -> TResult<T> + Send + 'static,
    {
        let (fut_tx, fut_rx) = futures::oneshot::<TResult<OpData>>();
        let tx_main = self.tx.clone();
        let thread_name = String::from(&self.name[..]);
        self.pool.execute(|| run().map(|x| x.to_opdata()))
            .and_then(move |res: TResult<OpData>| {
                Ok(tx_main.send(Box::new(move |_: TurtlWrap| { fut_tx.complete(res) })))
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

