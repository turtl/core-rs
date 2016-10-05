//! Define our error/result structs

use ::std::error::Error;
use ::std::convert::From;

use ::rusqlite::Error as SqlError;

quick_error! {
    #[derive(Debug)]
    /// Dumpy's main error object.
    pub enum DError {
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err.description())
        }
        SqlError(err: SqlError) {
            description(err.description())
            display("SQL error: {}", err.description())
        }
    }
}

/// A macro to make it easy to create From impls for DError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for DError {
            fn from(err: $t) -> DError {
                DError::Boxed(Box::new(err))
            }
        }
    )
}

from_err!(::jedi::JSONError);

impl From<SqlError> for DError {
    fn from(err: SqlError) -> DError {
        DError::SqlError(err)
    }
}

pub type DResult<T> = Result<T, DError>;

