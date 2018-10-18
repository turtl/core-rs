//! Define our error/result structs

use ::std::error::Error;
use ::std::convert::From;
use ::std::io::Error as IoError;

quick_error! {
    #[derive(Debug)]
    /// Clippo's main error object.
    pub enum CError {
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err.description())
        }
        Http(status: ::reqwest::StatusCode, msg: String) {
            description("HTTP error")
            display("http error ({}): {}", status.canonical_reason().unwrap_or("unknown"), msg)
        }
        Io(err: IoError) {
            cause(err)
            description("io error")
            display("io error: {}", err)
        }
        Url(err: ::url::ParseError) {
            cause(err)
            description("url parse error")
            display("url parse error: {}", err)
        }
        Selector(err: String) {
            description("selector parse error")
            display("selector parse error: {}", err)
        }
        Yaml(err: ::serde_yaml::Error) {
            cause(err)
            description("yaml error")
            display("yaml error: {}", err)
        }
    }
}

/// A macro to make it easy to create From impls for CError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for ::error::CError {
            fn from(err: $t) -> ::error::CError {
                CError::Boxed(Box::new(err))
            }
        }
    )
}

from_err!(::reqwest::Error);
from_err!(::reqwest::UrlError);

impl From<IoError> for CError {
    fn from(err: IoError) -> CError { CError::Io(err) }
}
impl From<::serde_yaml::Error> for CError {
    fn from(err: ::serde_yaml::Error) -> CError { CError::Yaml(err) }
}


pub type CResult<T> = Result<T, CError>;

