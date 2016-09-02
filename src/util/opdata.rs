//! OpData wraps some standard data formats we like to pass around to make it
//! easier for our async systems to convert from/to.
//!
//! TODO: rebuild using From

use ::util::json::Value;
use ::error::{TResult, TError};

#[derive(Debug)]
/// Holds data we return from Thredder instances that can be passed around and
/// converted back to its original type easily. This makes it so we can pass in
/// callbacks that return different types generically.
pub enum OpData {
    Bin(Vec<u8>),
    Str(String),
    JSON(Value),
    Null(()),
    VecStringPair((Vec<u8>, String)),   // weird, i know. don't judge me.
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
make_converter!((Vec<u8>, String), VecStringPair);


