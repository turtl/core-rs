use std::fs::File;
use std::path::Path;
use std::io::prelude::*;

use ::error::{TResult, TError};
use ::util::json::{self, Value, Deserialize};

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
    let data: Value = try_t!(json::parse(&contents));
    Ok(data)
}

#[allow(dead_code)]
/// get a string value from our config
pub fn get<T: Deserialize>(keys: &[&str]) -> TResult<T> {
    Ok(try_t!(json::get(keys, &*CONFIG)))
}

