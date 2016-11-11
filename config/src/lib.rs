extern crate jedi;
#[macro_use]
extern crate lazy_static;

use ::std::fs::File;
use ::std::path::Path;
use ::std::io::prelude::*;
use ::std::env;
use ::std::sync::RwLock;

use ::jedi::{JSONError, Value, Serialize, Deserialize};

pub type TResult<T> = Result<T, JSONError>;

lazy_static! {
    /// create a static/global CONFIG var, and load it with our config data
    static ref CONFIG: RwLock<Value> = {
        match load_config() {
            Ok(x) => RwLock::new(x),
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
    let mut file = File::open(&path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let data: Value = jedi::parse_yaml(&contents)?;
    Ok(data)
}

/// get a string value from our config
pub fn get<T: Deserialize>(keys: &[&str]) -> TResult<T> {
    let guard = (*CONFIG).read().unwrap();
    jedi::get(keys, &guard)
        .map_err(|e| From::from(e))
}

/// Set a value into our heroic config
pub fn set<T: Serialize>(keys: &[&str], val: &T) -> TResult<()> {
    let mut guard = (*CONFIG).write().unwrap();
    jedi::set(keys, &mut guard, val)
        .map_err(|e| From::from(e))
}

