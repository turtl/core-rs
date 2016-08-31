#[macro_use]
extern crate quick_error;
extern crate rusqlite;

use ::std::error::Error;

pub use ::rusqlite::Connection;

quick_error! {
    #[derive(Debug)]
    /// Defines our error class
    pub enum HarrError {
        SqlError(err: rusqlite::Error) {
            description(err.description())
            display("sqlite error: {}", err.description())
        }
    }
}

impl From<rusqlite::Error> for HarrError {
    fn from(err: rusqlite::Error) -> HarrError {
        HarrError::SqlError(err)
    }
}

pub type HResult<T> = Result<T, HarrError>;

/// The adapter is our shitty ORM interface around the SQLite connection
pub struct Adapter {
    conn: Connection
}

impl Adapter {
    /// Create a new harrrrsqlite adapter from an existing connection.
    fn new(conn: Connection) -> Adapter {
        Adapter {
            conn: conn
        }
    }

    /// Apply a schema to our database.
    ///
    /// This takes a JSON string and applies any changes in the database so it
    /// matches the schema.
    fn apply(schema: &String) -> HResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
