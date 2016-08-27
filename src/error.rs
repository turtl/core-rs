use ::std::error::Error;
use ::std::convert::From;

use ::futures::BoxFuture;
use ::hyper::status::StatusCode;

use ::crypto::CryptoError;
use ::util::json::JSONError;

quick_error! {
    #[derive(Debug)]
    pub enum TError {
        Shutdown {
            description("shutting down")
            display("shutting down")
        }
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err.description())
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

pub type TResult<T> = Result<T, TError>;
pub type TFutureResult<T> = BoxFuture<T, TError>;

/// converts non-TError errors to TError. this is a macro because I am sure this
/// is the "wrong" way to do this and once I know a better way, I can hopefully
/// fix it later
#[macro_export]
macro_rules! toterr {
    ($e:expr) => (TError::Boxed(Box::new($e)))
}

/// try!-esque wrapper around toterr
///
/// TODO: replace with From::from implementation for all generic errors (ie Msg)
#[macro_export]
macro_rules! try_t {
    ($e:expr) => (try!($e.map_err(|e| toterr!(e))))
}

