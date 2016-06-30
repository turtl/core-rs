use std::fs::File;
use std::path::Path;
use std::io::prelude::*;

use serde_json;
use serde_json::Value;
use ::error::{TResult, TError};
use ::util::json;

/// create a static/global CONFIG var, and load it with our config data
lazy_static! {
    static ref CONFIG: Value = {
        load_config().unwrap()
    };
}

/// load/parse our config file, and return the parsed JSON value
fn load_config() -> TResult<Value> {
    let path = Path::new("config.json");
    let mut file = try_t!(File::open(&path));
    let mut contents = String::new();
    try_t!(file.read_to_string(&mut contents));
    let data: Value = try_t!(serde_json::from_str(&contents));
    Ok(data)
}

/// get a string value from our config
#[allow(dead_code)]
pub fn get_str(keys: &[&str]) -> TResult<String> {
    match json::find_string(keys, &*CONFIG) {
        Ok(x) => Ok(x.to_owned()),
        Err(x) => Err(toterr!(x)),
    }
}

#[allow(dead_code)]
pub fn get_int(keys: &[&str]) -> TResult<i64> {
    match json::find_int(keys, &*CONFIG) {
        Ok(x) => Ok(x.clone()),
        Err(x) => Err(toterr!(x)),
    }
}

