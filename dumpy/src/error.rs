//! Define our error/result structs

use ::std::error::Error;
use ::std::convert::From;

use ::rusqlite::Error as SqlError;
use ::jedi::JSONError;

quick_error! {
    #[derive(Debug)]
    /// Dumpy's main error object.
    pub enum DError {
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        Boxed(err: Box<dyn Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err.description())
        }
        SqlError(err: SqlError) {
            description(err.description())
            display("SQL error: {}", err.description())
        }
        JSON(err: JSONError) {
            cause(err)
            description("JSON error")
            display("JSON error: {}", err)
        }
    }
}

impl From<SqlError> for DError {
    fn from(err: SqlError) -> DError {
        DError::SqlError(err)
    }
}

impl From<JSONError> for DError {
    fn from(err: JSONError) -> DError {
        DError::JSON(err)
    }
}

pub type DResult<T> = Result<T, DError>;

