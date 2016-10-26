//! This is a storage abstraction layer over SQLite.
//!
//! It provides a simple interface to "dump" JSON objects into SQLite and pull
//! them back out again. It actually stores all objects in one big table, and
//! has a secondary table that provides indexes. Thre are a few reasons for it
//! working like this:
//!
//!   1. It's simple. There's no "schema" ...we just send in a JSON object and
//!      it gets stringified and stored in the object body. Any fields we want
//!      to search on are indexed in the separate index table.
//!   2. Having indexes in a second table means we can do things like have
//!      multi-value indexes. So if you have an object, and you want to index
//!      each value of an array in that object, you just make a separate entry
//!      in the index table for each value, and point each one to your target
//!      object.
//!
//! All that said, unless this use-case fits yours perfectly, don't use this
//! library. It's interface could be thought of as a crude IndexedDB. It was
//! made specifically for the Turtl app and probably won't ever do the things
//! you want it to.

extern crate jedi;
#[macro_use]
extern crate quick_error;
extern crate rusqlite;
extern crate serde_json;

use ::rusqlite::Connection;
use ::rusqlite::types::Value as SqlValue;
use ::rusqlite::Error as SqlError;
use ::jedi::{Value, JSONError};

pub mod error;

pub use ::error::DError;
use ::error::DResult;

/// The Dumpy struct stores our schema and acts as a namespace for our public
/// functions.
pub struct Dumpy {
    schema: Value,
}

impl Dumpy {
    /// Create a new dumpy
    pub fn new(schema: Value) -> Dumpy {
        Dumpy {
            schema: schema,
        }
    }

    /// Init our dumpy store on an existing connection.
    pub fn init(&self, conn: &Connection) -> DResult<()> {
        try!(conn.execute("CREATE TABLE IF NOT EXISTS dumpy_objects (id VARCHAR(64) PRIMARY KEY, table_name VARCHAR(32), data TEXT)", &[]));
        try!(conn.execute("CREATE TABLE IF NOT EXISTS dumpy_index (id INTEGER PRIMARY KEY, table_name VARCHAR(32), index_name VARCHAR(32), vals VARCHAR(256), object_id VARCHAR(64))", &[]));
        try!(conn.execute("CREATE TABLE IF NOT EXISTS dumpy_kv (key VARCHAR(32) PRIMARY KEY, value TEXT)", &[]));

        try!(conn.execute("CREATE INDEX IF NOT EXISTS dumpy_idx_index ON dumpy_index (table_name, index_name, vals)", &[]));
        try!(conn.execute("CREATE INDEX IF NOT EXISTS dumpy_idx_index_obj ON dumpy_index (table_name, object_id)", &[]));
        try!(conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS dumpy_idx_kv ON dumpy_kv (key)", &[]));
        Ok(())
    }

    /// Store an object!
    pub fn store(&self, conn: &Connection, table: &String, obj: &Value) -> DResult<()> {
        let id = try!(jedi::get::<String>(&["id"], obj));
        let json = try!(jedi::stringify(obj));
        // "upsert" the object
        try!(conn.execute("INSERT OR REPLACE INTO dumpy_objects (id, table_name, data) VALUES ($1, $2, $3)", &[&id, table, &json]));
        // wipte out all indexes for this object
        try!(conn.execute("DELETE FROM dumpy_index WHERE table_name = $1 AND object_id = $2", &[table, &id]));

        let indexes = match jedi::get::<Vec<Value>>(&[table, "indexes"], &self.schema) {
            Ok(x) => x,
            Err(e) => match e {
                JSONError::DeadEnd | JSONError::NotFound(..) => {
                    Vec::new()
                },
                _ => return Err(From::from(e)),
            }
        };
        for index in &indexes {
            let fields = try!(jedi::get::<Vec<String>>(&["fields"], index));
            let idx_name: String = match jedi::get::<String>(&["name"], index) {
                Ok(x) => x,
                Err(e) => match e {
                    JSONError::DeadEnd | JSONError::NotFound(_) => {
                        let mut name = fields[0].clone();
                        for field in &fields[1..] {
                            name = format!("{}_{}", name, field);
                        }
                        name
                    }
                    _ => return Err(From::from(e)),
                }
            };
            let mut val_vec: Vec<Vec<String>> = Vec::new();
            let blankval = String::from("");

            // build an array of an array of values (we want all combinations
            // of the various fields)
            for field in &fields {
                let val = jedi::walk(&[&field], &obj);
                let mut subvals: Vec<String> = Vec::new();
                match val {
                    Ok(x) => {
                        match *x {
                            Value::String(ref x) => {
                                subvals.push(x.clone());
                            },
                            Value::I64(ref x) => {
                                subvals.push(format!("{}", x));
                            },
                            Value::U64(ref x) => {
                                subvals.push(format!("{}", x));
                            },
                            Value::F64(ref x) => {
                                subvals.push(format!("{}", x));
                            },
                            Value::Bool(ref x) => {
                                subvals.push(format!("{}", x));
                            },
                            Value::Array(ref x) => {
                                for val in x {
                                    match *val {
                                        Value::String(ref s) => {
                                            subvals.push(s.clone());
                                        }
                                        Value::I64(x) => {
                                            subvals.push(format!("{}", x));
                                        },
                                        Value::U64(x) => {
                                            subvals.push(format!("{}", x));
                                        },
                                        Value::F64(x) => {
                                            subvals.push(format!("{}", x));
                                        },
                                        _ => {
                                            subvals.push(blankval.clone());
                                        },
                                    }
                                }
                            },
                            Value::Null | Value::Object(_) => {
                                subvals.push(blankval.clone());
                            },
                        }
                    },
                    Err(JSONError::NotFound(_)) => {
                        subvals.push(blankval.clone());
                    },
                    Err(e) => return Err(From::from(e)),
                }
                val_vec.push(subvals);
            }

            fn combine(acc: String, next: &Vec<Vec<String>>, final_vals: &mut Vec<String>) {
                if next.len() == 0 {
                    final_vals.push(acc);
                    return;
                }
                let here = &next[0];
                let next = Vec::from(&next[1..]);
                for val in here {
                    let acced;
                    if acc == "" {
                        acced = format!("{}", val);
                    } else {
                        acced = format!("{}|{}", acc, val);
                    }
                    combine(acced, &next, final_vals);
                }

            }
            let mut vals: Vec<String> = Vec::new();
            combine(String::from(""), &val_vec, &mut vals);
            for val in &vals {
                try!(conn.execute("INSERT INTO dumpy_index (table_name, index_name, vals, object_id) VALUES ($1, $2, $3, $4)", &[
                    table,
                    &idx_name,
                    val,
                    &id,
                ]));
            }
        }
        Ok(())
    }

    /// Remove all traces of an object.
    pub fn delete(&self, conn: &Connection, table: &String, id: &String) -> DResult<()> {
        try!(conn.execute("DELETE FROM dumpy_objects WHERE table_name = $1 AND id = $2", &[table, id]));
        try!(conn.execute("DELETE FROM dumpy_index WHERE table_name = $1 AND object_id = $2", &[table, id]));
        Ok(())
    }

    /// Get an object from dumpy's store
    pub fn get(&self, conn: &Connection, table: &String, id: &String) -> DResult<Option<Value>> {
        let query = "SELECT data FROM dumpy_objects WHERE id = $1 AND table_name = $2";
        let res = conn.query_row_and_then(query, &[id, table], |row| -> DResult<Value> {
            let data: SqlValue = try!(row.get_checked("data"));
            match data {
                SqlValue::Text(ref x) => {
                    Ok(try!(jedi::parse(x)))
                },
                _ => Err(DError::Msg(format!("dumpy: {}: {}: `data` field is not a string", table, id))),
            }
        });
        match res {
            Ok(x) => Ok(Some(x)),
            Err(e) => match e {
                DError::SqlError(e) => match e {
                    SqlError::QueryReturnedNoRows => Ok(None),
                    _ => Err(From::from(e)),
                },
                _ => Err(e),
            },
        }
    }

    /// Find objects using a given index/values
    pub fn find(&self, conn: &Connection, table: &String, index: &String, vals: &Vec<String>) -> DResult<Vec<Value>> {
        let mut query = try!(conn.prepare("SELECT object_id FROM dumpy_index WHERE table_name = $1 AND index_name = $2 AND vals LIKE $3"));
        let vals_str = vals
            .into_iter()
            .fold(String::new(), |acc, x| {
                if acc == "" {
                    format!("{}", x)
                } else {
                    format!("{}|{}", acc, x)
                }
            });
        let vals_str = format!("{}%", vals_str);
        let rows = try!(query.query_map(&[table, index, &vals_str], |row| {
            row.get("object_id")
        }));
        let mut ids: Vec<String> = Vec::new();
        for oid in rows {
            ids.push(try!(oid));
        }

        let oids = ids.into_iter().fold(String::new(), |acc, x| {
            if acc == "" {
                format!("'{}'", x)
            } else {
                format!("{}, '{}'", acc, x)
            }
        });
        let query = format!("SELECT data FROM dumpy_objects WHERE id IN ({}) ORDER BY id ASC", oids);
        let mut query = try!(conn.prepare(&query[..]));
        let rows = try!(query.query_map(&[], |row| {
            row.get("data")
        }));
        let mut objects: Vec<Value> = Vec::new();
        for data in rows {
            objects.push(try!(jedi::parse(&try!(data))));
        }
        Ok(objects)
    }

    /// Get ALL objects in a table
    pub fn all(&self, conn: &Connection, table: &String) -> DResult<Vec<Value>> {
        let query = "SELECT data FROM dumpy_objects WHERE table_name = ? ORDER BY id ASC";
        let mut query = try!(conn.prepare(query));
        let rows = try!(query.query_map(&[table], |row| {
            row.get("data")
        }));
        let mut objects: Vec<Value> = Vec::new();
        for data in rows {
            objects.push(try!(jedi::parse(&try!(data))));
        }
        Ok(objects)
    }

    /// Set a value into the key/val store
    pub fn kv_set(&self, conn: &Connection, key: &str, val: &String) -> DResult<()> {
        try!(conn.execute("INSERT OR REPLACE INTO dumpy_kv (key, value) VALUES ($1, $2)", &[&key, val]));
        Ok(())
    }

    /// Get a value from the key/val store
    pub fn kv_get(&self, conn: &Connection, key: &str) -> DResult<Option<String>> {
        let query = "SELECT value FROM dumpy_kv WHERE key = $1";
        let res = conn.query_row_and_then(query, &[&key], |row| -> DResult<String> {
            let data: SqlValue = try!(row.get_checked("value"));
            match data {
                SqlValue::Text(x) => {
                    Ok(x)
                },
                _ => Err(DError::Msg(format!("dumpy: kv: {}: `value` field is not a string", key))),
            }
        });
        match res {
            Ok(x) => Ok(Some(x)),
            Err(e) => match e {
                DError::SqlError(e) => match e {
                    SqlError::QueryReturnedNoRows => Ok(None),
                    _ => Err(From::from(e)),
                },
                _ => Err(e),
            },
        }
    }

    /// Remove a k/v val
    pub fn kv_delete(&self, conn: &Connection, key: &str) -> DResult<()> {
        try!(conn.execute("DELETE FROM dumpy_kv WHERE key = $1", &[&key]));
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi;
    use ::rusqlite::Connection;
    use ::rusqlite::types::Value as SqlValue;
    use ::error::DResult;

    fn pre_test() -> (Connection, Dumpy) {
        let conn = Connection::open_in_memory().unwrap();
        let schema = jedi::parse(&String::from(r#"{"boards":null,"notes":{"indexes":[{"fields":["boards"]},{"name":"user_boards","fields":["user_id","boards"]}]}}"#)).unwrap();
        let dumpy = Dumpy::new(schema);
        (conn, dumpy)
    }

    fn index_count(conn: &Connection) -> i64 {
        conn.query_row_and_then("SELECT COUNT(*) AS count FROM dumpy_index", &[], |row| -> DResult<i64> {
            let data: SqlValue = try!(row.get_checked("count"));
            match data {
                SqlValue::Integer(ref x) => Ok(x.clone()),
                _ => Err(DError::Msg(format!("error grabbing count"))),
            }
        }).unwrap()
    }

    #[test]
    fn inits() {
        let (conn, dumpy) = pre_test();
        dumpy.init(&conn).unwrap();
    }

    #[test]
    fn stores_stuff_gets_stuff() {
        let (conn, dumpy) = pre_test();
        let note = jedi::parse(&String::from(r#"{"id":"abc123","user_id":"andrew123","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        dumpy.init(&conn).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note).unwrap();
        let note = dumpy.get(&conn, &String::from("notes"), &String::from("abc123")).unwrap().unwrap();
        assert_eq!(jedi::get::<String>(&["id"], &note).unwrap(), "abc123");
        assert_eq!(jedi::get::<String>(&["user_id"], &note).unwrap(), "andrew123");
        assert_eq!(jedi::get::<Vec<String>>(&["boards"], &note).unwrap(), vec![String::from("1234"), String::from("5678")]);
        assert_eq!(jedi::get::<String>(&["body"], &note).unwrap(), "this is my note lol");
    }

    #[test]
    fn upserts() {
        let (conn, dumpy) = pre_test();
        dumpy.init(&conn).unwrap();
        let note = jedi::parse(&String::from(r#"{"id":"abc123","user_id":"andrew123","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note).unwrap();
        assert_eq!(index_count(&conn), 4);
        let note = jedi::parse(&String::from(r#"{"id":"abc123","user_id":"hellp","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note).unwrap();
        assert_eq!(index_count(&conn), 4);
        let note = jedi::parse(&String::from(r#"{"id":"abc123","user_id":"getajob","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note).unwrap();
        assert_eq!(index_count(&conn), 4);
        let note = dumpy.get(&conn, &String::from("notes"), &String::from("abc123")).unwrap().unwrap();
        assert_eq!(jedi::get::<String>(&["id"], &note).unwrap(), "abc123");
        assert_eq!(jedi::get::<String>(&["user_id"], &note).unwrap(), "getajob");
    }

    #[test]
    fn deletes_stuff() {
        let (conn, dumpy) = pre_test();
        let note1 = jedi::parse(&String::from(r#"{"id":"n0mnm","user_id":"3443","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        let note2 = jedi::parse(&String::from(r#"{"id":"6tuns","user_id":"9823","boards":["1234","2222"],"body":"this is my note lol"}"#)).unwrap();
        dumpy.init(&conn).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note1).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note2).unwrap();
        assert!(dumpy.get(&conn, &String::from("notes"), &String::from("6tuns")).unwrap().is_some());
        assert!(dumpy.get(&conn, &String::from("notes"), &String::from("n0mnm")).unwrap().is_some());
        assert_eq!(index_count(&conn), 8);
        dumpy.delete(&conn, &String::from("notes"), &String::from("n0mnm")).unwrap();
        assert!(dumpy.get(&conn, &String::from("notes"), &String::from("6tuns")).unwrap().is_some());
        assert!(dumpy.get(&conn, &String::from("notes"), &String::from("n0mnm")).unwrap().is_none());
        assert_eq!(index_count(&conn), 4);
    }

    #[test]
    fn indexes_and_searches() {
        let (conn, dumpy) = pre_test();
        let note1 = jedi::parse(&String::from(r#"{"id":"n0mnm","user_id":"3443","boards":["1234","5678"],"body":"this is my note lol"}"#)).unwrap();
        let note2 = jedi::parse(&String::from(r#"{"id":"6tuns","user_id":"9823","boards":["1234","2222"],"body":"this is my note lol"}"#)).unwrap();
        let note3 = jedi::parse(&String::from(r#"{"id":"p00pz","user_id":"9823","boards":["5896"],"body":"this is my note lol"}"#)).unwrap();
        let note4 = jedi::parse(&String::from(r#"{"id":"l4cky","user_id":"2938","boards":["3385", "4247"],"body":"this is my note lol"}"#)).unwrap();
        let note5 = jedi::parse(&String::from(r#"{"id":"h4iry","user_id":"4187","boards":["1234"],"body":"this is my note lol"}"#)).unwrap();
        let note6 = jedi::parse(&String::from(r#"{"id":"scl0c","user_id":"4187","body":"this is my note lol"}"#)).unwrap();
        let note7 = jedi::parse(&String::from(r#"{"id":"gr1my","body":"this is my note lol"}"#)).unwrap();
        let board1 = jedi::parse(&String::from(r#"{"id":"s4nd1","title":"get a job"}"#)).unwrap();
        let board2 = jedi::parse(&String::from(r#"{"id":"s4nd2","title":null}"#)).unwrap();
        dumpy.init(&conn).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note1).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note2).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note3).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note4).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note5).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note6).unwrap();
        dumpy.store(&conn, &String::from("notes"), &note7).unwrap();
        dumpy.store(&conn, &String::from("boards"), &board1).unwrap();
        dumpy.store(&conn, &String::from("boards"), &board2).unwrap();

        let notes = dumpy.find(&conn, &String::from("notes"), &String::from("user_boards"), &vec![String::from("9823"), String::from("1234")]).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(jedi::get::<String>(&["id"], &notes[0]).unwrap(), "6tuns");

        let notes = dumpy.find(&conn, &String::from("notes"), &String::from("user_boards"), &vec![String::from("9823")]).unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(jedi::get::<String>(&["id"], &notes[0]).unwrap(), "6tuns");
        assert_eq!(jedi::get::<String>(&["id"], &notes[1]).unwrap(), "p00pz");

        let notes = dumpy.find(&conn, &String::from("notes"), &String::from("boards"), &vec![String::from("1234")]).unwrap();
        assert_eq!(notes.len(), 3);
        assert_eq!(jedi::get::<String>(&["id"], &notes[0]).unwrap(), "6tuns");
        assert_eq!(jedi::get::<String>(&["id"], &notes[1]).unwrap(), "h4iry");
        assert_eq!(jedi::get::<String>(&["id"], &notes[2]).unwrap(), "n0mnm");

        let all_records = dumpy.all(&conn, &String::from("notes")).unwrap();
        assert_eq!(all_records.len(), 7);
    }

    #[test]
    fn kv_set_get() {
        let (conn, dumpy) = pre_test();
        dumpy.init(&conn).unwrap();
        dumpy.kv_set(&conn, "some_setting", &String::from("I AM ABOVE THE LAW")).unwrap();
        let val = dumpy.kv_get(&conn, "some_setting").unwrap();
        assert_eq!(val.unwrap(), "I AM ABOVE THE LAW");

        dumpy.kv_set(&conn, "some_setting", &String::from("i got no feelin'")).unwrap();
        let val = dumpy.kv_get(&conn, "some_setting").unwrap();
        assert_eq!(val.unwrap(), "i got no feelin'");

        let val = dumpy.kv_get(&conn, "doesnt_exist").unwrap();
        assert_eq!(val, None);

        dumpy.kv_delete(&conn, "some_setting").unwrap();
        let val = dumpy.kv_get(&conn, "some_setting").unwrap();
        assert_eq!(val, None);
    }
}
