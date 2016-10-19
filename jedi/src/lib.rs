/// A collection of utilities for dealing with JSON and YAML objects.

#[macro_use]
extern crate quick_error;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;

use ::std::error::Error;
use ::std::convert::From;

use ::serde_json::Error as SerdeJsonError;
use ::serde_yaml::Error as SerdeYamlError;
pub use ::serde_json::Value;
pub use ::serde::de::Deserialize;
pub use ::serde::ser::Serialize;

quick_error! {
    #[derive(Debug)]
    pub enum JSONError {
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("json: error: {}", format!("{}", err))
        }
        Parse(err: serde_json::Error) {
            cause(err)
            description("parse error")
            display("json: parse error: {}", err)
        }
        Stringify(err: serde_json::Error) {
            cause(err)
            description("stringify error")
            display("json: stringify error: {}", err)
        }
        DeadEnd {
            description("dead end")
            display("json: lookup dead end")
        }
        NotFound(key: String) {
            description("key not found")
            display("json: key not found: {}", key)
        }
        InvalidKey(key: String) {
            description("invalid key")
            display("json: invalid key for object: {}", key)
        }
    }
}

pub type JResult<T> = Result<T, JSONError>;

/// A macro to make it easy to create From impls for JSONError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for JSONError {
            fn from(err: $t) -> JSONError {
                JSONError::Boxed(Box::new(err))
            }
        }
    )
}

from_err!(::std::io::Error);
from_err!(SerdeJsonError);
from_err!(SerdeYamlError);

/// Parse a JSON string and return a Result<Value>
pub fn parse<T: Deserialize>(string: &String) -> JResult<T> {
    serde_json::from_str(string).map_err(JSONError::Parse)
}

/// Parse a YAML string and return a Value type
pub fn parse_yaml(string: &String) -> JResult<Value> {
    let data: Value = try!(serde_yaml::from_str(string));
    Ok(data)
}

/// Turn a JSON-serializable object into a Result<String> of JSON.
pub fn stringify<T: Serialize>(obj: &T) -> JResult<String> {
    serde_json::to_string(&obj).map_err(|e| JSONError::Stringify(e))
}

/// Turn a JSON-serializable object into a Result<Value>
pub fn to_val<T: Serialize>(obj: &T) -> Value {
    serde_json::to_value(obj)
}

/// Turn a JSON Value into a object that implements Deserialize
pub fn from_val<T: Deserialize>(val: Value) -> JResult<T> {
    serde_json::from_value(val).map_err(|e| JSONError::Stringify(e))
}

/// Walk a JSON structure, given a key path. Traverses both objects and arrays,
/// returning a reference to the found value, if any.
///
/// # Examples
///
/// ```
/// let json_str = String::from(r#"{"user":{"name":"barky"}}"#);
/// let parsed = json::parse(&json_str);
/// let nameval = walk(&["user", "name"], &parsed).unwrap();
/// ```
pub fn walk<'a>(keys: &[&str], data: &'a Value) -> JResult<&'a Value> {
    let last: bool = keys.len() == 0;
    if last { return Ok(data); }

    let key = keys[0];

    match *data {
        Value::Object(ref obj) => {
            match obj.get(key) {
                Some(d) => walk(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        Value::Array(ref arr) => {
            let ukey = match key.parse::<usize>() {
                Ok(x) => x,
                Err(..) => return Err(JSONError::InvalidKey(key.to_owned())),
            };
            match arr.get(ukey) {
                Some(d) => walk(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        _ => return Err(JSONError::DeadEnd),
    }
}

#[allow(dead_code)]
/// Walk a JSON structure, given a key path. Traverses both objects and arrays,
/// returning a reference to the found value, if any. This function takes and
/// returns a mutable reference to the Value.
///
/// # Examples
///
/// ```
/// let json_str = String::from(r#"{"user":{"name":"barky"}}"#);
/// let parsed = json::parse(&json_str);
/// let nameval = walk(&["user", "name"], &parsed).unwrap();
/// ```
pub fn walk_mut<'a>(keys: &[&str], data: &'a mut Value) -> JResult<&'a mut Value> {
    let last: bool = keys.len() == 0;
    if last { return Ok(data); }

    let key = keys[0];

    match *data {
        Value::Object(ref mut obj) => {
            match obj.get_mut(key) {
                Some(d) => walk_mut(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        Value::Array(ref mut arr) => {
            let ukey = match key.parse::<usize>() {
                Ok(x) => x,
                Err(..) => return Err(JSONError::InvalidKey(key.to_owned())),
            };
            match arr.get_mut(ukey) {
                Some(d) => walk_mut(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        _ => return Err(JSONError::DeadEnd),
    }
}

/// Like `walk`, except that this returns the raw type instead of a Value. How
/// lovely?
///
/// # Examples
///
/// ```
/// let json_str = String::from(r#"{"user":{"name":"barky"}}"#);
/// let parsed = json::parse(&json_str);
/// let name = get(&["user", "name"], &parsed).unwrap();
/// println!("name is {}", name);
/// ```
pub fn get<T: Deserialize>(keys: &[&str], value: &Value) -> JResult<T> {
    match walk(keys, value) {
        Ok(ref x) => {
            match serde_json::from_value((*x).clone()) {
                Ok(x) => Ok(x),
                Err(e) => Err(JSONError::NotFound(format!("get: {:?}: {}", keys, e))),
            }
        },
        Err(e) => Err(e),
    }
}

#[allow(dead_code)]
/// Set a field into a mutable JSON Value
pub fn set<T: Serialize>(keys: &[&str], container: &mut Value, to: &T) -> JResult<()> {
    if keys.len() == 0 {
        return Err(JSONError::InvalidKey(format!("set: no keys given")));
    }

    let butlast = &keys[0..(keys.len() - 1)];
    let last = (keys[(keys.len() - 1)..])[0];

    let mut val = try!(walk_mut(butlast, container));
    match *val {
        Value::Object(ref mut x) => {
            x.insert(String::from(last), to_val(to));
            Ok(())
        },
        Value::Array(ref mut x) => {
            let ukey = match last.parse::<usize>() {
                Ok(x) => x,
                Err(..) => return Err(JSONError::InvalidKey(last.to_owned())),
            };
            *(&mut x[ukey]) = to_val(to);
            Ok(())
        },
        _ => Err(JSONError::DeadEnd),
    }
}

/// Returns a blank Value hash/object type
pub fn obj() -> Value {
    Value::Object(::std::collections::BTreeMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_json() -> String {
        String::from(r#"["test",{"name":"slappy","age":17,"has_friends":false},2,3.885]"#)
    }

    fn get_parsed() -> Value {
        parse(&get_json()).unwrap()
    }

    #[test]
    fn can_parse() {
        get_parsed();
    }

    #[test]
    fn can_get_value() {
        let val_str: String = get(&["0"], &get_parsed()).unwrap();
        let val_int: i64 = get(&["1", "age"], &get_parsed()).unwrap();
        let val_float: f64 = get(&["3"], &get_parsed()).unwrap();
        let val_bool: bool = get(&["1", "has_friends"], &get_parsed()).unwrap();

        assert_eq!(val_str, "test");
        assert_eq!(val_int, 17);
        assert_eq!(val_float, 3.885);
        assert_eq!(val_bool, false);
    }
}

