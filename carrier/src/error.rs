//! Define our error/result structs

use ::std::error::Error;
use ::std::convert::From;

use ::rusqlite::Error as SqlError;

quick_error! {
    #[derive(Debug)]
    /// Dumpy's main error object.
    pub enum CError {
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        SqlError(err: SqlError) {
            description(err.description())
            display("SQL error: {}", err.description())
        }
    }
}

impl From<SqlError> for CError {
    fn from(err: SqlError) -> CError {
        CError::SqlError(err)
    }
}

pub type CResult<T> = Result<T, CError>;

