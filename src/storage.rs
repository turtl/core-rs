//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::sync::Arc;

use ::crypto;
use ::rusqlite::Connection;
use ::rusqlite::types::{ToSql, Null, sqlite3_stmt};
use ::libc::c_int;
use ::jedi::{self, Value};
use ::dumpy::Dumpy;

use ::models::model::{self, ModelDataRef};
use ::models::protected::Protected;

use ::error::{TResult, TError};

/// Make ModelDataRef a ToSql type
impl<'a> ToSql for ModelDataRef<'a> {
    unsafe fn bind_parameter(&self, stmt: *mut sqlite3_stmt, col: c_int) -> c_int {
        match *self {
            ModelDataRef::Bool(ref x) => {
                match *x {
                    Some(val) => val.bind_parameter(stmt, col),
                    None => Null.bind_parameter(stmt, col),
                }
            },
            ModelDataRef::I64(ref x) => {
                match *x {
                    Some(val) => val.bind_parameter(stmt, col),
                    None => Null.bind_parameter(stmt, col),
                }
            },
            ModelDataRef::F64(ref x) => {
                match *x {
                    Some(val) => val.bind_parameter(stmt, col),
                    None => Null.bind_parameter(stmt, col),
                }
            },
            ModelDataRef::String(ref x) => {
                match *x {
                    Some(val) => val.bind_parameter(stmt, col),
                    None => Null.bind_parameter(stmt, col),
                }
            },
            ModelDataRef::Bin(ref x) => {
                match *x {
                    Some(val) => val.bind_parameter(stmt, col),
                    /*
                    Some(val) => {
                        match crypto::to_base64(val) {
                            Ok(val) => val.bind_parameter(stmt, col),
                            Err(..) => Null.bind_parameter(stmt, col),
                        }
                    },
                    */
                    None => Null.bind_parameter(stmt, col),
                }
            },
            ModelDataRef::List(ref x) => {
                match *x {
                    Some(val) => match jedi::stringify(val) {
                        Ok(val) => val.bind_parameter(stmt, col),
                        Err(_) => Null.bind_parameter(stmt, col),
                    },
                    None => Null.bind_parameter(stmt, col),
                }
            },
        }
    }
}

/// Make sure we have a client ID, and sync it with the model system
pub fn setup_client_id(storage: &Storage) -> TResult<()> {
    let conn = &storage.conn;
    let dumpy = &storage.dumpy;
    let id = match try!(dumpy.kv_get(conn, "client_id")) {
        Some(x) => x,
        None => {
            let client_id = try!(crypto::random_hash());
            try!(dumpy.kv_set(conn, "client_id", &client_id));
            client_id
        },
    };
    model::set_client_id(id)
}

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    conn: Connection,
    dumpy: Arc<Dumpy>,
}

impl Storage {
    /// Make a Storage lol
    pub fn new(location: &String, schema: Value) -> TResult<Storage> {
        let conn = try!(if location == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(location)
        });

        // set up dumpy
        let dumpy = Arc::new(Dumpy::new(schema));
        try!(dumpy.init(&conn));

        Ok(Storage {
            conn: conn,
            dumpy: dumpy,
        })
    }

    /// Run a query
    pub fn run<F, T>(&self, run: F) -> TResult<T>
        where F: FnOnce(&Connection) -> TResult<T> + Sync + Send + 'static
    {
        run(&self.conn)
    }

    /// Save a model to our db. Make sure it's serialized before handing it in.
    pub fn save<T>(&self, model: &T) -> TResult<()>
        where T: Protected
    {
        let modeldata = model.untrusted_data();
        let dumpy = self.dumpy.clone();
        let table = model.table();

        self.run(move |conn| -> TResult<()> {
            dumpy.store(&conn, &table, &modeldata)
                .map_err(|e| From::from(e))
        })
    }

    /// Get a model's data by id
    pub fn get<T>(&self, model: &T) -> TResult<Value>
        where T: Protected
    {
        let id = model.id().map(|x| x.clone());
        let table = model.table();
        let dumpy = self.dumpy.clone();

        self.run(move |conn| -> TResult<Value> {
            let id: String = match id {
                Some(x) => x,
                None => return Err(TError::MissingField(format!("Storage::get() -- model missing `id` field"))),
            };
            dumpy.get(&conn, &table, &id)
                .map_err(|e| From::from(e))
        })
    }
}

unsafe impl Send for Storage {}
unsafe impl Sync for Storage {}

#[cfg(test)]
mod tests {
    use super::*;

    use ::models::model::{self, Model};
    use ::models::protected::Protected;

    protected!{
        pub struct Shiba {
            ( color: String ),
            ( name: String,
              tags: Vec<String> ),
            ( )
        }
    }

    #[test]
    fn runs_queries() {
    }

    #[test]
    fn saves_models() {
    }
}
