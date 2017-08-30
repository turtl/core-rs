use ::std::error::Error;
use ::std::io::Error as IoError;
use ::std::convert::From;
use ::std::sync::Arc;

use ::futures::BoxFuture;
use ::hyper::status::StatusCode;
use ::jedi::JSONError;
use ::dumpy::DError;

use ::crypto::CryptoError;

quick_error! {
    #[derive(Debug)]
    /// Turtl's main error object.
    pub enum TError {
        Wrapped(function: &'static str, file: &'static str, line: u32, err: Arc<TError>) {
            description("Turtl wrap error")
            display("{{\"file\":\"{}\",\"line\":{},\"err\":\"{}\"}}", function, file, line, err)
        }
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err)
        }
        Msg(msg: String) {
            description(msg)
            display("error: {}", msg)
        }
        BadValue(msg: String) {
            description(msg)
            display("bad value: {}", msg)
        }
        MissingField(msg: String) {
            description(msg)
            display("missing field: {}", msg)
        }
        MissingData(msg: String) {
            description(msg)
            display("missing data: {}", msg)
        }
        MissingCommand(msg: String) {
            description(msg)
            display("unknown command: {}", msg)
        }
        NotFound(msg: String) {
            description(msg)
            display("not found: {}", msg)
        }
        PermissionDenied(msg: String) {
            description(msg)
            display("permission denied: {}", msg)
        }
        ConnectionRequired {
            description("connection required")
            display("connection required")
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
        Dumpy(err: DError) {
            cause(err)
            description("Dumpy error")
            display("Dumpy error: {}", err)
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

impl TError {
    /// Shed this TError object's tough, icy outer shell to reveal it's true
    /// sensitive inner-self.
    ///
    /// It's best to call this function when you are in a *safe place* and
    /// surrounded by people you love.
    pub fn shed(self) -> TError {
        match self {
            TError::Wrapped(function, file, line, wrappederr) => {
                match Arc::try_unwrap(wrappederr) {
                    Ok(x) => x.shed(),
                    Err(y) => TError::Wrapped(function, file, line, y),
                }
            }
            _ => self,
        }
    }
}

/// Define a macro that, if and when the time is right, returns a static string
/// of the current function.
macro_rules! function {
    () => {{
        "<unimplemented>"
    }}
}

/// A macro that wraps creation of errors so we get file/line info for debugging
#[macro_export]
macro_rules! twrap {
    ($terror:expr) => {
        ::error::TError::Wrapped(function!(), file!(), line!(), ::std::sync::Arc::new($terror))
    }
}

/// Invokes twrap! while also wrapping in Err
#[macro_export]
macro_rules! TErr {
    ($terror:expr) => { Err(twrap!($terror)) }
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
            let err: ::error::TError = From::from($e);
            twrap!(err)
        }
    )
}

/// A macro to make it easy to create From impls for TError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for ::error::TError {
            fn from(err: $t) -> ::error::TError {
                if cfg!(feature = "panic-on-error") {
                    panic!("{:?}", err);
                } else {
                    TError::Boxed(Box::new(err))
                }
            }
        }
    )
}

impl From<CryptoError> for TError {
    fn from(err: CryptoError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Crypto(err)
        }
    }
}
impl From<IoError> for TError {
    fn from(err: IoError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Io(err)
        }
    }
}
impl From<JSONError> for TError {
    fn from(err: JSONError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            match err {
                JSONError::Boxed(x) => TError::Boxed(x),
                _ => TError::JSON(err),
            }
        }
    }
}
impl From<DError> for TError {
    fn from(err: DError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            match err {
                DError::Boxed(x) => TError::Boxed(x),
                _ => TError::Dumpy(err),
            }
        }
    }
}
impl From<Box<::std::any::Any + Send>> for TError {
    fn from(err: Box<::std::any::Any + Send>) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Msg(format!("{:?}", err))
        }
    }
}
from_err!(::fern::InitError);
from_err!(::carrier::CError);
from_err!(::clouseau::CError);
from_err!(::std::string::FromUtf8Error);
from_err!(::rusqlite::Error);
from_err!(::std::num::ParseIntError);
from_err!(::hyper::Error);
from_err!(::hyper::error::ParseError);
from_err!(::regex::Error);
from_err!(::std::sync::mpsc::RecvError);
from_err!(::glob::PatternError);
from_err!(::glob::GlobError);

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

/// Like Ok, but for boxed futures (goes great with TFutureResult)
#[macro_export]
macro_rules! FOk {
    ($ex:expr) => {
        ::futures::finished($ex).boxed();
    }
}

/// Like Err, but for boxed futures (goes great with TFutureResult)
#[macro_export]
macro_rules! FErr {
    ($ex:expr) => {
        ::futures::failed(From::from($ex)).boxed();
    }
}

/// A helper to make trying stuff in futures easier
#[macro_export]
macro_rules! ftry {
    ($ex:expr) => {
        match $ex {
            Ok(x) => x,
            Err(e) => return FErr!(e),
        }
    }
}

