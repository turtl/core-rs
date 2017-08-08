//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::sync::{Arc, RwLock};
use ::std::mem;

use ::crypto;
use ::rusqlite::{self, Connection};
use ::jedi::{self, Value};
use ::dumpy::Dumpy;
use ::config;

use ::models::model::{self};
use ::models::protected::Protected;
use ::models::storable::Storable;

use ::error::{TResult, TError};

/// Given a db filename, return the foll path we'll use for the db file
pub fn db_location(db_name: &String) -> TResult<String> {
    if cfg!(test) {
        return Ok(String::from(":memory:"))
    }
    let data_folder = config::get::<String>(&["data_folder"])?;
    let db_location = if data_folder == ":memory:" {
        String::from(":memory:")
    } else {
        format!("{}/{}.sqlite", data_folder, db_name)
    };
    Ok(db_location)
}

/// Make sure we have a client ID, and sync it with the model system
pub fn setup_client_id(storage: Arc<RwLock<Storage>>) -> TResult<()> {
    let storage_guard = storage.read().unwrap();
    let conn = &storage_guard.conn;
    let dumpy = &storage_guard.dumpy;
    let id = match dumpy.kv_get(conn, "client_id")? {
        Some(x) => x,
        None => {
            let client_id = crypto::random_hash()?;
            dumpy.kv_set(conn, "client_id", &client_id)?;
            client_id
        },
    };
    model::set_client_id(id)
}

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    pub conn: Connection,
    pub dumpy: Dumpy,
}

impl Storage {
    /// Make a Storage lol
    pub fn new(location: &String, schema: Value) -> TResult<Storage> {
        // open in multi-threaded mode: we can have the same db open in multiple
        // threads as long as each thread has its own connection:
        //   https://www.sqlite.org/threadsafe.html
        let flags =
            rusqlite::SQLITE_OPEN_READ_WRITE |
            rusqlite::SQLITE_OPEN_CREATE |
            rusqlite::SQLITE_OPEN_NO_MUTEX |
            rusqlite::SQLITE_OPEN_URI;
        let conn = if location == ":memory:" {
            Connection::open_in_memory_with_flags(flags)
        } else {
            Connection::open_with_flags(location, flags)
        }?;

        // set up dumpy
        let dumpy = Dumpy::new(schema);
        dumpy.init(&conn)?;

        Ok(Storage {
            conn: conn,
            dumpy: dumpy,
        })
    }

    /// Save a model to our db. Make sure it's serialized before handing it in.
    pub fn save<T>(&self, model: &T) -> TResult<()>
        where T: Protected + Storable
    {
        let modeldata = model.data_for_storage()?;
        let table = model.table();

        Ok(self.dumpy.store(&self.conn, &String::from(table), &modeldata)?)
    }

    /// Get a model's data by id
    #[allow(dead_code)]
    pub fn get<T>(&self, table: &str, id: &String) -> TResult<Option<T>>
        where T: Protected + Storable
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
        where T: Protected + Storable
    {
        let id = match model.id() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("storage::destroy() -- missing `id` field"))),
        };
        let table = model.table();
        Ok(self.dumpy.delete(&self.conn, &String::from(table), &id)?)
    }

    /// Grab all values from a "table"
    pub fn all<T>(&self, table: &str) -> TResult<Vec<T>>
        where T: Protected + Storable
    {
        Ok(jedi::from_val(Value::Array(self.dumpy.all(&self.conn, &String::from(table))?))?)
    }

    /// Find values by index/value in a "table"
    pub fn find<T>(&self, table: &str, index: &str, vals: &Vec<String>) -> TResult<Vec<T>>
        where T: Protected + Storable
    {
        Ok(jedi::from_val(Value::Array(self.dumpy.find(&self.conn, &String::from(table), &String::from(index), vals)?))?)
    }

    /// Get ALL objects in a table with the given IDs
    pub fn by_id<T>(&self, table: &str, ids: &Vec<String>) -> TResult<Vec<T>>
        where T: Protected + Storable
    {
        Ok(jedi::from_val(Value::Array(self.dumpy.by_id(&self.conn, &String::from(table), &ids)?))?)
    }

    /// Grab a value from our dumpy k/v store
    pub fn kv_get(&self, key: &str) -> TResult<Option<String>> {
        Ok(self.dumpy.kv_get(&self.conn, key)?)
    }

    /// Set a value into our dumpy k/v store
    pub fn kv_set(&self, key: &str, val: &String) -> TResult<()> {
        Ok(self.dumpy.kv_set(&self.conn, key, val)?)
    }

    pub fn kv_delete(&self, key: &str) -> TResult<()> {
        Ok(self.dumpy.kv_delete(&self.conn, key)?)
    }

    /// Close the db connection
    pub fn close(&mut self) -> TResult<()> {
        let mut conn = Connection::open_in_memory()?;
        mem::swap(&mut self.conn, &mut conn);
        conn.close()?;
        Ok(())
    }
}

// NOTE: since we open our db connection in full-mutex mode, we can safely pass
// it around between threads willy-nilly.
unsafe impl Sync for Storage {}

#[cfg(test)]
mod tests {
    use super::*;

    use ::jedi::{self, Value};
    use ::rusqlite::types::Value as SqlValue;

    use ::error::TResult;
    use ::models::model::{self, Model};
    use ::models::protected::Protected;

    protected! {
        #[derive(Serialize, Deserialize)]
        pub struct Shiba {
            #[protected_field(public)]
            pub color: Option<String>,

            #[protected_field(private)]
            pub name: Option<String>,
            #[protected_field(private)]
            pub tags: Option<Vec<String>>,
        }
    }
    make_storable!(Shiba, "shibas");

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
        let mut model = Shiba::new_with_id().unwrap();
        let key = model.generate_key().unwrap().clone();
        model.color = Some(String::from("sesame"));
        model.name = Some(String::from("Kofi"));
        model.tags = Some(vec![String::from("serious")]);
        model.serialize().unwrap();
        storage.save(&model).unwrap();

        let id = model.id().unwrap();
        let mut shiba2: Shiba = storage.get("shibas", id).unwrap().unwrap();
        shiba2.set_key(Some(key));
        shiba2.deserialize().unwrap();
        assert_eq!(shiba2.color.unwrap(), String::from("sesame"));
        assert_eq!(shiba2.name.unwrap(), String::from("Kofi"));
        assert_eq!(shiba2.tags.unwrap(), vec![String::from("serious")]);

        assert_eq!(storage.all::<Shiba>("shibas").unwrap().len(), 1);
    }

    #[test]
    fn deletes_models() {
        let storage = pretest();
        let mut model = Shiba::new_with_id().unwrap();
        model.generate_key().unwrap();
        model.color = Some(String::from("sesame"));
        model.name = Some(String::from("Kofi"));
        model.tags = Some(vec![String::from("serious")]);
        model.serialize().unwrap();
        storage.save(&model).unwrap();

        storage.delete(&model).unwrap();

        let id = model.id().unwrap();
        let sheeb: Option<Shiba> = storage.get("shibas", id).unwrap();
        assert!(sheeb.is_none());
    }

    #[test]
    fn kv_stuff() {
        // ^kv stuff? were the midterms hard?
        let storage = pretest();
        assert_eq!(storage.kv_get("get a job").unwrap(), None);
        storage.kv_set("get a job", &String::from("no way")).unwrap();
        assert_eq!(storage.kv_get("get a job").unwrap().unwrap(), "no way");
        storage.kv_delete("get a job").unwrap();
        assert_eq!(storage.kv_get("get a job").unwrap(), None);
    }
}

