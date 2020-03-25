use ::std::error::Error;
use ::std::io::Error as IoError;
use ::std::convert::From;
use ::std::sync::Arc;
use ::jedi::{Value, JSONError};
use ::dumpy::DError;
use ::clippo::error::CError as ClippoError;
use ::migrate::error::MError as MigrateError;
use ::rusqlite;
use ::api::{APIError, StatusCode};
use ::crypto::CryptoError;
use ::util;

macro_rules! quick_error_obj {
    ($ty:expr, $err:expr) => {
        json!({"type": $ty, "message": util::json_or_string(format!("{}", $err))})
    }
}

quick_error! {
    #[derive(Debug)]
    /// Turtl's main error object.
    pub enum TError {
        Wrapped(function: &'static str, file: &'static str, line: u32, err: Arc<TError>) {
            description("Turtl wrap error")
            display("{}", json!({"file": file, "line": line, "err": util::json_or_string(format!("{}", err)), "wrapped": true}))
        }
        Boxed(err: Box<dyn Error + Send + Sync>) {
            description(err.description())
            display("{}", quick_error_obj!("generic", err))
        }
        Msg(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("generic", msg))
        }
        Panic(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("panic", msg))
        }
        BadValue(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("bad_value", msg))
        }
        MissingField(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("missing_field", msg))
        }
        MissingData(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("missing_data", msg))
        }
        MissingCommand(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("missing_command", msg))
        }
        NotFound(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("not_found", msg))
        }
        PermissionDenied(msg: String) {
            description(msg)
            display("{}", quick_error_obj!("permission_denied", msg))
        }
        Validation(objtype: String, errors: Vec<(String, String)>) {
            description("validaton error")
            display("{}", json!({"type": "validation", "subtype": objtype, "errors": errors}))
        }
        ConnectionRequired {
            description("connection required")
            display("{}", json!({"type": "connection_required"}))
        }
        Crypto(err: CryptoError) {
            cause(err)
            description("crypto error")
            display("{}", quick_error_obj!("crypto_error", err))
        }
        JSON(err: JSONError) {
            cause(err)
            description("JSON error")
            display("{}", quick_error_obj!("json_error", err))
        }
        Dumpy(err: DError) {
            cause(err)
            description("Dumpy error")
            display("{}", quick_error_obj!("dumpy_error", err))
        }
        Clippo(err: ClippoError) {
            cause(err)
            description("Clippo error")
            display("{}", quick_error_obj!("clippy_error", err))
        }
        Migrate(err: MigrateError) {
            cause(err)
            description("migrate error")
            display("{}", quick_error_obj!("migrate_error", err))
        }
        Io(err: IoError) {
            cause(err)
            description("io error")
            display("{}", quick_error_obj!("io_error", err))
        }
        Api(status: StatusCode, msg: Value) {
            description("API error")
            display("{}", json!({"type": "api", "subtype": status.canonical_reason().unwrap_or("unknown"), "message": msg}))
        }
        Http(status: StatusCode, msg: Value) {
            description("HTTP error")
            display("{}", json!({"type": "http", "subtype": status.canonical_reason().unwrap_or("unknown"), "message": msg}))
        }
        ParseError(msg: String) {
            description("Parse error")
            display("{}", quick_error_obj!("parse_error", msg))
        }
        TryAgain {
            description("try again")
            display("{}", json!({"type": "try_again"}))
        }
        NotImplemented {
            description("not implemented")
            display("{}", json!({"type": "not_implemented"}))
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
impl From<ClippoError> for TError {
    fn from(err: ClippoError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Clippo(err)
        }
    }
}
impl From<MigrateError> for TError {
    fn from(err: MigrateError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Migrate(err)
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
impl From<APIError> for TError {
    fn from(err: APIError) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            match err {
                APIError::Boxed(x) => TError::Boxed(x),
                APIError::Api(status, msg) => TError::Api(status, msg),
                APIError::Io(err) => TError::Io(err),
                APIError::Msg(err) => TError::Msg(format!("Api Error: {}", err)),
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
impl From<Box<dyn (::std::any::Any) + Send>> for TError {
    fn from(err: Box<dyn (::std::any::Any) + Send>) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err);
        } else {
            TError::Msg(format!("{:?}", err))
        }
    }
}
impl From<(rusqlite::Connection, rusqlite::Error)> for TError {
    fn from(err: (rusqlite::Connection, rusqlite::Error)) -> TError {
        if cfg!(feature = "panic-on-error") {
            panic!("{:?}", err.1);
        } else {
            TError::Boxed(Box::new(err.1))
        }
    }
}
from_err!(::fern::InitError);
from_err!(::carrier::CError);
from_err!(::clouseau::CError);
from_err!(::std::string::FromUtf8Error);
from_err!(::rusqlite::Error);
from_err!(::std::num::ParseIntError);
from_err!(::regex::Error);
from_err!(::std::sync::mpsc::RecvError);
from_err!(::glob::PatternError);
from_err!(::glob::GlobError);
from_err!(::log::SetLoggerError);
from_err!(::url::ParseError);

pub type BoxFuture<T, E> = Box<dyn (::futures::Future<Item = T, Error = E>) + Send>;
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
        Box::new(::futures::finished($ex))
    }
}

/// Like Err, but for boxed futures (goes great with TFutureResult)
#[macro_export]
macro_rules! FErr {
    ($ex:expr) => {
        Box::new(::futures::failed(From::from($ex)))
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

