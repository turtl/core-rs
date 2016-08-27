use std::fs::File;
use std::path::Path;
use std::io::prelude::*;

use ::error::TResult;
use ::util::json::{self, Value, Deserialize};

lazy_static! {
    /// create a static/global CONFIG var, and load it with our config data
    static ref CONFIG: Value = {
        load_config().unwrap()
    };
}

/// load/parse our config file, and return the parsed JSON value
fn load_config() -> TResult<Value> {
    let path = Path::new("config.yaml");
    let mut file = try!(File::open(&path));
    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));
    let data: Value = try!(json::parse_yaml(&contents));
    Ok(data)
}

#[allow(dead_code)]
/// get a string value from our config
pub fn get<T: Deserialize>(keys: &[&str]) -> TResult<T> {
    Ok(try!(json::get(keys, &*CONFIG)))
}

