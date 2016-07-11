#[macro_use]
pub mod protected;
pub mod user;
pub mod file;
pub mod note;

use ::error::{TResult};
use ::util::json;

pub trait Model {
    /// Grab *all* data for this model (safe or not)
    fn data(&self) -> json::Value;

    /// Get a value from this model
    fn get<T>(&self, field: &str) -> Option<T>
        where T: json::Serialize + json::Deserialize
    {

        match json::get(&[field], &self.data()) {
            Ok(x) => Some(x),
            Err(..) => None,
        }
    }

    fn set<T>(&mut self, field: &str, value: T) -> TResult<()>
        where T: json::Serialize + json::Deserialize
    {
        Ok(())
    }
}

