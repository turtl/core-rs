use ::serde_json::Value;
use ::serde_json::Value::{Object, Array};

quick_error! {
    #[derive(Debug)]
    pub enum JSONError {
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
            description("invalid string")
            display("json: the value found is not an int")
        }
    }
}

pub type JResult<T> = Result<T, JSONError>;

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

pub fn find_int<'a>(keys: &[&str], data: &'a Value) -> JResult<&'a i64> {
    return match find(&keys, &data) {
        Ok(x) => match *x {
            Value::I64(ref x) => Ok(x),
            _ => Err(JSONError::InvalidInt),
        },
        Err(e) => Err(e),
    }
}

