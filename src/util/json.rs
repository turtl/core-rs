use ::serde_json;
pub use ::serde_json::Value;
use ::serde_json::Value::{Object, Array};

quick_error! {
    #[derive(Debug)]
    pub enum JSONError {
        Parse(err: serde_json::Error) {
            cause(err)
            description("parse error")
            display("json: parse error: {}", err)
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
        InvalidString {
            description("invalid string")
            display("json: the value found is not a string")
        }
        InvalidInt {
            description("invalid int")
            display("json: the value found is not an int")
        }
        InvalidFloat {
            description("invalid float")
            display("json: the value found is not a float")
        }
        InvalidBool {
            description("invalid bool")
            display("json: the value found is not a bool")
        }
    }
}

pub type JResult<T> = Result<T, JSONError>;

pub fn parse(string: &String) -> JResult<Value> {
    let data: Value = try!(serde_json::from_str(string).map_err(JSONError::Parse));
    Ok(data)
}

pub fn find<'a>(keys: &[&str], data: &'a Value) -> JResult<&'a Value> {
    let last: bool = keys.len() == 0;
    if last { return Ok(data); }

    let key = keys[0];

    match *data {
        Object(ref obj) => {
            match obj.get(key) {
                Some(d) => find(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        Array(ref arr) => {
            let ukey = match key.parse::<usize>() {
                Ok(x) => x,
                Err(..) => return Err(JSONError::InvalidKey(key.to_owned())),
            };
            match arr.get(ukey) {
                Some(d) => find(&keys[1..].to_vec(), d),
                None => Err(JSONError::NotFound(key.to_owned())),
            }
        },
        _ => return Err(JSONError::DeadEnd),
    }
}

pub fn find_string<'a>(keys: &[&str], data: &'a Value) -> JResult<&'a String> {
    return match find(&keys, &data) {
        Ok(x) => match *x {
            Value::String(ref x) => Ok(x),
            _ => Err(JSONError::InvalidString),
        },
        Err(e) => Err(e),
    }
}

pub fn find_int(keys: &[&str], data: &Value) -> JResult<i64> {
    return match find(&keys, &data) {
        Ok(x) => match *x {
            Value::I64(x) => Ok(x),
            Value::U64(x) => Ok(x as i64),
            _ => Err(JSONError::InvalidInt),
        },
        Err(e) => Err(e),
    }
}

pub fn find_float(keys: &[&str], data: &Value) -> JResult<f64> {
    return match find(&keys, &data) {
        Ok(x) => match *x {
            Value::F64(x) => Ok(x),
            _ => Err(JSONError::InvalidFloat),
        },
        Err(e) => Err(e),
    }
}

pub fn find_bool(keys: &[&str], data: &Value) -> JResult<bool> {
    return match find(&keys, &data) {
        Ok(x) => match *x {
            Value::Bool(x) => Ok(x),
            _ => Err(JSONError::InvalidBool),
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
    fn can_find() {
        let parsed = get_parsed();
        let found = find(&["1", "name"], &parsed).unwrap();

        match *found {
            Value::String(ref x) => assert_eq!(*x, "slappy"),
            _ => panic!("value not found"),
        }
    }

    #[test]
    fn can_find_string() {
        assert_eq!(find_string(&["0"], &get_parsed()).unwrap(), "test");
    }

    #[test]
    fn can_find_int() {
        assert_eq!(find_int(&["1", "age"], &get_parsed()).unwrap(), 17i64);
    }

    #[test]
    fn can_find_float() {
        assert_eq!(find_float(&["3"], &get_parsed()).unwrap(), 3.885f64);
    }

    #[test]
    fn can_find_bool() {
        assert_eq!(find_bool(&["1", "has_friends"], &get_parsed()).unwrap(), false);
    }
}

