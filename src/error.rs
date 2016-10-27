use ::std::error::Error;
use ::std::convert::From;

use ::futures::BoxFuture;
use ::hyper::status::StatusCode;
use ::jedi::JSONError;

use ::crypto::CryptoError;

quick_error! {
    #[derive(Debug)]
    /// Turtl's main error object.
    pub enum TError {
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err)
        }
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        BadValue(str: String) {
            description(str)
            display("bad value: {}", str)
        }
        MissingField(str: String) {
            description(str)
            display("missing field: {}", str)
        }
        MissingData(str: String) {
            description(str)
            display("missing data: {}", str)
        }
        MissingCommand(str: String) {
            description(str)
            display("unknown command: {}", str)
        }
        CryptoError(err: CryptoError) {
            cause(err)
            description("crypto error")
            display("crypto error: {}", err)
        }
        ApiError(status: StatusCode) {
            description("API error")
            display("api error: {}", status.canonical_reason().unwrap_or("unknown"))
        }
        TryAgain {
            description("try again")
            display("try again")
        }
        NotImplemented {
            description("not implemented")
            display("not implemented")
        }
    }
}

/// converts non-TError errors to TError, via the From trait. This means that
/// we can't do blanket conversions of errors anymore (like the good ol' days)
/// but instead must provide a Err -> TError From implementation. This is made
/// much easier by the from_err! macro below, although hand-written conversions
/// are welcome as well.
#[macro_export]
macro_rules! toterr {
    ($e:expr) => (
        {
            let err: TError = From::from($e);
            err
        }
    )
}

/// A macro to make it easy to create From impls for TError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for TError {
            fn from(err: $t) -> TError {
                TError::Boxed(Box::new(err))
            }
        }
    )
}

impl From<CryptoError> for TError {
    fn from(err: CryptoError) -> TError {
        TError::CryptoError(err)
    }
}
impl From<JSONError> for TError {
    fn from(err: JSONError) -> TError {
        match err {
            JSONError::Boxed(x) => TError::Boxed(x),
            _ => TError::Boxed(Box::new(err)),
        }
    }
}
impl From<Box<::std::any::Any + Send>> for TError {
    fn from(err: Box<::std::any::Any + Send>) -> TError {
        TError::Msg(format!("{:?}", err))
    }
}
from_err!(::std::io::Error);
from_err!(::fern::InitError);
from_err!(::carrier::CError);
from_err!(::std::string::FromUtf8Error);
from_err!(::rusqlite::Error);
from_err!(::std::num::ParseIntError);
from_err!(::hyper::Error);
from_err!(::dumpy::DError);

pub type TResult<T> = Result<T, TError>;
pub type TFutureResult<T> = BoxFuture<T, TError>;

/// A helper to make reporting errors easier
#[macro_export]
macro_rules! try_or {
    ($ex:expr, $sym:ident, $err:expr) => {
        match $ex {
            Ok(_) => (),
            Err($sym) => {
                $err;
            },
        }
    }
}

