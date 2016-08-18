//! Thredder is a thread tracking system that not only creates threads for
//! specific purposes, but sets up communication channels between the threads
//! and tracks the state of them.

use ::std::sync::mpsc::Sender;
use ::std::marker::Send;

use ::futures::{Future, Oneshot};
use ::futures_cpupool::CpuPool;

use ::error::{TResult, TError};
use ::util::json::Value;

#[derive(Debug)]
pub enum OpData {
    Bin(Vec<u8>),
    Str(String),
    JSON(Value),
}

/// A simple trait for allowing easy conversion from data into OpData
pub trait OpConverter : Sized {
    /// Convert a piece of data into an OpData enum
    fn to_opdata(self) -> TResult<OpData>;

    /// Convert an OpData back to its raw form
    fn to_value(TResult<OpData>) -> Self;
}

impl OpData {
    /// Convert an OpData into its raw contained self
    pub fn to_value<T>(val: TResult<OpData>) -> T
        where T: OpConverter
    {
        T::to_value(val)
    }
}

/// Makes creating conversions between Type -> OpData and back again easy
macro_rules! make_converter {
    ($conv_type:ty, $enumfield:ident) => (
        impl OpConverter for TResult<$conv_type> {
            fn to_opdata(self) -> TResult<OpData> {
                self.map(|x| OpData::$enumfield(x))
            }

            fn to_value(data: TResult<OpData>) -> Self {
                match data {
                    Ok(x) => match x {
                        OpData::$enumfield(x) => Ok(x),
                        _ => Err(TError::BadValue(format!("OpConverter: problem converting {}", stringify!($conv_type)))),
                    },
                    Err(e) => Err(e),
                }
            }
        }
    )
}

make_converter!(Vec<u8>, Bin);
make_converter!(String, Str);
make_converter!(Value, JSON);

/// Creates a way to call a Box<FnOnce> basically
pub trait Thunk: Send + 'static {
    fn call_box(self: Box<Self>);
}
impl<F: FnOnce() + Send + 'static> Thunk for F {
    fn call_box(self: Box<Self>) {
        (*self)();
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
    pub fn run<F, C, T>(&self, run: F, handler: C)
        where T: OpConverter + Send + 'static,
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

