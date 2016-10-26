//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::sync::Arc;

use ::crypto;
use ::rusqlite::{self, Connection};
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
pub fn setup_client_id(storage: Arc<Storage>) -> TResult<()> {
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
    pub conn: Connection,
    pub dumpy: Arc<Dumpy>,
}

impl Storage {
    /// Make a Storage lol
    pub fn new(location: &String, schema: Value) -> TResult<Storage> {
        let flags =
            rusqlite::SQLITE_OPEN_READ_WRITE |
            rusqlite::SQLITE_OPEN_CREATE |
            rusqlite::SQLITE_OPEN_FULL_MUTEX |
            rusqlite::SQLITE_OPEN_URI;
        let conn = try!(if location == ":memory:" {
            Connection::open_in_memory_with_flags(flags)
        } else {
            Connection::open_with_flags(location, flags)
        });

        // set up dumpy
        let dumpy = Arc::new(Dumpy::new(schema));
        try!(dumpy.init(&conn));

        Ok(Storage {
            conn: conn,
            dumpy: dumpy,
        })
    }

    /// Save a model to our db. Make sure it's serialized before handing it in.
    pub fn save<T>(&self, model: &T) -> TResult<()>
        where T: Protected
    {
        let modeldata = model.untrusted_data();
        let table = model.table();

        self.dumpy.store(&self.conn, &table, &modeldata)
            .map_err(|e| From::from(e))
    }

    /// Get a model's data by id
    pub fn get<T>(&self, table: &str, id: &String) -> TResult<Option<T>>
        where T: Protected
    {
        match self.dumpy.get(&self.conn, &String::from(table), id) {
            Ok(x) => match x {
                Some(x) => {
                    let res = match jedi::from_val(x) {
                        Ok(x) => x,
                        Err(e) => return Err(From::from(e)),
                    };
                    Ok(Some(res))
                }
                None => Ok(None),
            },
            Err(e) => Err(From::from(e)),
        }
    }

    /// Delete a model from storage
    pub fn delete<T>(&self, model: &T) -> TResult<()>
        where T: Protected
    {
        let id = match model.id() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("storage::destroy() -- missing `id` field"))),
        };
        let table = model.table();
        self.dumpy.delete(&self.conn, &table, &id)
            .map_err(|e| From::from(e))
    }

    /// Grab all values from a "table"
    pub fn all(&self, table: &str) -> TResult<Vec<Value>> {
        self.dumpy.all(&self.conn, &String::from(table))
            .map_err(|e| From::from(e))
    }

    /// Grab a value from our dumpy k/v store
    pub fn kv_get(&self, key: &str) -> TResult<Option<String>> {
        self.dumpy.kv_get(&self.conn, key)
            .map_err(|e| From::from(e))
    }

    /// Set a value into our dumpy k/v store
    pub fn kv_set(&self, key: &str, val: &String) -> TResult<()> {
        self.dumpy.kv_set(&self.conn, key, val)
            .map_err(|e| From::from(e))
    }
}

// NOTE: since we open our db connection in full-mutex mode, we can safely pass
// it around between threads willy-nilly.
unsafe impl Send for Storage {}
unsafe impl Sync for Storage {}

#[cfg(test)]
mod tests {
    use super::*;

    use ::jedi::{self, Value};
    use ::rusqlite::types::Value as SqlValue;

    use ::error::TResult;
    use ::models::model::{self, Model};
    use ::models::protected::Protected;
    use ::crypto;

    protected!{
        pub struct Shiba {
            ( color: String ),
            ( name: String,
              tags: Vec<String> ),
            ( )
        }
    }

    fn pretest() -> Storage {
        model::set_client_id(String::from("c0f4c762af6c42e4079cced2dfe16b4d010b190ad75ade9d83ff8cee0e96586d")).unwrap();
        let schema_str = r#"{"notes":{"indexes":[{"fields":["user_id"]},{"fields":["boards"]}]}}"#;
        let schema: Value = jedi::parse(&String::from(schema_str)).unwrap();
        Storage::new(&String::from(":memory:"), schema).unwrap()
    }

    #[test]
    fn runs_queries() {
        let storage = pretest();
        storage.conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name VARCHAR(16))", &[]).unwrap();
        storage.conn.execute("INSERT INTO test (name) VALUES ($1)", &[&String::from("bartholomew")]).unwrap();
        let then = "SELECT * FROM test LIMIT 1";
        let res = storage.conn.query_row_and_then(then, &[], |row| -> TResult<String> {
            let name_sql: SqlValue = row.get_checked("name").unwrap();
            match name_sql {
                SqlValue::Text(ref x) => Ok(x.clone()),
                _ => panic!("bad dates (name field was not a string)"),
            }
        }).unwrap();
        assert_eq!(res, "bartholomew");
    }

    #[test]
    fn saves_retrieves_models() {
        let storage = pretest();
        let mut model = Shiba::new();
        let key = Vec::from(&(model.generate_key().unwrap())[..]);
        model.set("color", String::from("sesame")).unwrap();
        model.set("name", String::from("Kofi")).unwrap();
        model.set("tags", vec![String::from("serious")]).unwrap();
        model.serialize().unwrap();
        storage.save(&model).unwrap();

        let id = model.id().unwrap();
        let mut shiba2: Shiba = storage.get("shiba", id).unwrap().unwrap();
        shiba2.key = Some(key);
        shiba2.deserialize().unwrap();
        assert_eq!(shiba2.get::<String>("color").unwrap(), "sesame");
        assert_eq!(shiba2.get::<String>("name").unwrap(), "Kofi");
        assert_eq!(shiba2.get::<Vec<String>>("tags").unwrap(), &vec![String::from("serious")]);

        assert_eq!(storage.all("shiba").unwrap().len(), 1);
    }

    #[test]
    fn deletes_models() {
        let storage = pretest();
        let mut model = Shiba::new();
        model.generate_key().unwrap();
        model.set("color", String::from("sesame")).unwrap();
        model.set("name", String::from("Kofi")).unwrap();
        model.set("tags", vec![String::from("serious")]).unwrap();
        model.serialize().unwrap();
        storage.save(&model).unwrap();

        storage.delete(&model).unwrap();

        let id = model.id().unwrap();
        let sheeb: Option<Shiba> = storage.get("shiba", id).unwrap();
        assert!(sheeb.is_none());
    }

    #[test]
    fn kv_stuff() {
        // ^kv stuff? were the midterms hard?
        let storage = pretest();
        assert_eq!(storage.kv_get("get a job").unwrap(), None);
        storage.kv_set("get a job", &String::from("no way")).unwrap();
        assert_eq!(storage.kv_get("get a job").unwrap().unwrap(), "no way");
    }
}

