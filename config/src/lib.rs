extern crate jedi;
#[macro_use]
extern crate lazy_static;

use ::std::fs::File;
use ::std::path::Path;
use ::std::io::prelude::*;
use ::std::env;

use ::jedi::{JSONError, Value, Deserialize};

pub type TResult<T> = Result<T, JSONError>;

lazy_static! {
    /// create a static/global CONFIG var, and load it with our config data
    static ref CONFIG: Value = {
        match load_config() {
            Ok(x) => x,
            Err(e) => {
                panic!("error loading config: {}", e);
            },
        }
    };
}

/// load/parse our config file, and return the parsed JSON value
fn load_config() -> TResult<Value> {
    let path_env = match env::var("TURTL_CONFIG_FILE") {
        Ok(x) => x,
        Err(_) => String::from("config.yaml"),
    };
    let path = Path::new(&path_env[..]);
    let mut file = try!(File::open(&path));
    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));
    let data: Value = try!(jedi::parse_yaml(&contents));
    Ok(data)
}

#[allow(dead_code)]
/// get a string value from our config
pub fn get<T: Deserialize>(keys: &[&str]) -> TResult<T> {
    Ok(try!(jedi::get(keys, &*CONFIG)))
}

