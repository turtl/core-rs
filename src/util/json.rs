use ::serde_json;
pub use ::serde_json::Value;
pub use ::serde::de::Deserialize;
pub use ::serde::ser::Serialize;
use ::serde_yaml;

quick_error! {
    #[derive(Debug)]
    pub enum JSONError {
        Custom(str: String) {
            description("error")
            display("json: error: {}", str)
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

/// Parse a JSON string and return a Result<Value>
pub fn parse(string: &String) -> JResult<Value> {
    let data: Value = try!(serde_json::from_str(string).map_err(JSONError::Parse));
    Ok(data)
}

pub fn parse_yaml(string: &String) -> JResult<Value> {
    let data: Value = try!(serde_yaml::from_str(string).map_err(|e| JSONError::Custom(format!("yaml parse error: {}", e))));
    Ok(data)
}

/// Turn a JSON-serializable object into a Result<String> of JSON.
pub fn stringify<T>(obj: &T) -> JResult<String>
    where T: Serialize
{
    serde_json::to_string(&obj).map_err(|e| JSONError::Stringify(e))
}

/// Turn a JSON-serializable object into a Result<Value>
pub fn to_val<T>(obj: &T) -> Value
    where T: Serialize
{
    serde_json::to_value(obj)
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
                Err(e) => Err(JSONError::NotFound(format!("get: {}", e))),
            }
        },
        Err(e) => Err(e),
    }
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
        let json = get_json();
        parse(&json).unwrap();
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

