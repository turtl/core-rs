use ::std::error::Error;
use ::std::io::Error as IoError;
use ::std::convert::From;

use ::hyper::status::StatusCode;
use ::jedi::JSONError;

use ::crypto::CryptoError;

quick_error! {
    #[derive(Debug)]
    /// Turtl's main error object.
    pub enum MError {
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
        NotFound(str: String) {
            description(str)
            display("not found: {}", str)
        }
        Crypto(err: CryptoError) {
            cause(err)
            description("crypto error")
            display("crypto error: {}", err)
        }
        JSON(err: JSONError) {
            cause(err)
            description("JSON error")
            display("JSON error: {}", err)
        }
        Io(err: IoError) {
            cause(err)
            description("io error")
            display("io error: {}", err)
        }
        Api(status: StatusCode, msg: String) {
            description("API error")
            display("api error ({}): {}", status.canonical_reason().unwrap_or("unknown"), msg)
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

/// converts non-MError errors to MError, via the From trait. This means that
/// we can't do blanket conversions of errors anymore (like the good ol' days)
/// but instead must provide a Err -> MError From implementation. This is made
/// much easier by the from_err! macro below, although hand-written conversions
/// are welcome as well.
#[macro_export]
macro_rules! tomerr {
    ($e:expr) => (
        {
            let err: MError = From::from($e);
            err
        }
    )
}

/// A macro to make it easy to create From impls for MError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for MError {
            fn from(err: $t) -> MError {
                MError::Boxed(Box::new(err))
            }
        }
    )
}

impl From<CryptoError> for MError {
    fn from(err: CryptoError) -> MError {
        MError::Crypto(err)
    }
}
impl From<IoError> for MError {
    fn from(err: IoError) -> MError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            MError::Io(err)
        }
    }
}
impl From<JSONError> for MError {
    fn from(err: JSONError) -> MError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            match err {
                JSONError::Boxed(x) => MError::Boxed(x),
                _ => MError::JSON(err),
            }
        }
    }
}
impl From<Box<::std::any::Any + Send>> for MError {
    fn from(err: Box<::std::any::Any + Send>) -> MError {
        MError::Msg(format!("{:?}", err))
    }
}
from_err!(::fern::InitError);
from_err!(::std::string::FromUtf8Error);
from_err!(::std::num::ParseIntError);
from_err!(::hyper::Error);
from_err!(::hyper::error::ParseError);

pub type MResult<T> = Result<T, MError>;

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

