#[macro_use]
pub mod protected;
pub mod user;
pub mod file;
pub mod note;

use ::error::{TError, TResult};
use ::util::json;
use ::util::event;

pub trait Model {
    /// Grab *all* data for this model (safe or not)
    fn data(&self) -> &json::Value;

    /// Grab *all* data for this model (safe or not) as mutable
    fn data_mut(&mut self) -> &mut json::Value;

    /// Clear out the data in this Model
    fn clear(&mut self) -> ();

    /// Reset the data in this Model with a new Value
    fn reset(&mut self, data: json::Value) -> TResult<()>;

    /// Get this model's id
    fn id<T>(&self) -> Option<T>
        where T: json::Serialize + json::Deserialize
    {
        self.get("id")
    }

    /// Get a nested value from this model's data
    fn get_nest<T>(&self, fields: &[&str]) -> Option<T>
        where T: json::Serialize + json::Deserialize
    {
        match json::get(fields, self.data()) {
            Ok(x) => Some(x),
            Err(..) => None,
        }
    }

    /// Get a nested Value from this model's data
    fn get_nest_mut(&mut self, fields: &[&str]) -> Option<&mut json::Value> {
        match json::walk_mut(fields, self.data_mut()) {
            Ok(x) => Some(x),
            Err(..) => None,
        }
    }

    /// Set a nested value into this model's data
    fn set_nest<T>(&mut self, fields: &[&str], value: &T) -> TResult<()>
        where T: json::Serialize + json::Deserialize
    {
        json::set(fields, self.data_mut(), value)
            .map_err(|e| toterr!(e))
    }

    /// Get a value from this model's data
    fn get<T>(&self, field: &str) -> Option<T>
        where T: json::Serialize + json::Deserialize
    {
        self.get_nest(&[field])
    }

    /// Get a mutable Value from this model's data
    fn get_mut(&mut self, field: &str) -> Option<&mut json::Value> {
        self.get_nest_mut(&[field])
    }

    /// Set a value into this model's data
    fn set<T>(&mut self, field: &str, value: &T) -> TResult<()>
        where T: json::Serialize + json::Deserialize
    {
        self.set_nest(&[field], value)
    }

    /// Turn this model into a JSON string
    fn stringify(&self) -> TResult<String> {
        json::stringify(self.data())
            .map_err(|e| toterr!(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json;
    use ::std::collections::HashMap;
    use ::error::{TError};

    struct Rabbit {
        _data: json::Value,
    }

    impl Rabbit {
        fn new() -> Rabbit {
            Rabbit { _data: json::obj() }
        }
    }

    impl Model for Rabbit {
        fn data(&self) -> &json::Value {
            &self._data
        }

        fn data_mut(&mut self) -> &mut json::Value {
            &mut self._data
        }

        fn clear(&mut self) -> () {
            self._data = json::obj();
        }

        fn reset(&mut self, data: ::util::json::Value) -> ::error::TResult<()> {
            match data {
                json::Value::Object(..) => {
                    self._data = data;
                    Ok(())
                }
                _ => Err(TError::BadValue(String::from("Model::reset(): `data` is not an object type"))),
            }
        }
    }

    #[test]
    fn get_set() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.get::<Option<String>>("name"), None);
        rabbit.set("name", &String::from("Moussier")).unwrap();
        rabbit.set("phrase", &String::from("Startups HATE him!!!!1")).unwrap();
        rabbit.set("location", &{
            let mut hash: HashMap<String, String> = HashMap::new();
            hash.insert(String::from("city"), String::from("santa cruz"));
            hash
        }).unwrap();
        assert_eq!(rabbit.get::<String>("name").unwrap(), "Moussier");
        assert_eq!(rabbit.get::<String>("phrase").unwrap(), "Startups HATE him!!!!1");
        assert_eq!(rabbit.get_nest::<String>(&["location", "city"]).unwrap(), "santa cruz");
    }

    #[test]
    fn ids() {
        let mut rabbit = Rabbit::new();
        rabbit.set("id", &String::from("696969")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "696969");
    }

    #[test]
    fn clears() {
        let mut rabbit = Rabbit::new();
        rabbit.set("id", &String::from("omglol")).unwrap();
        rabbit.set("name", &String::from("hoppy")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "omglol");
        assert_eq!(rabbit.get::<String>("name").unwrap(), "hoppy");
        rabbit.clear();
        assert_eq!(rabbit.id::<String>(), None);
        assert_eq!(rabbit.get::<String>("name"), None);
    }

    #[test]
    fn resets() {
        let mut rabbit1 = Rabbit::new();
        let mut rabbit2 = Rabbit::new();
        rabbit1.set("id", &String::from("omglol")).unwrap();
        rabbit1.set("name", &String::from("hoppy")).unwrap();
        rabbit2.reset(rabbit1.data().clone()).unwrap();
        assert_eq!(rabbit2.id::<String>().unwrap(), "omglol");
        assert_eq!(rabbit2.get::<String>("name").unwrap(), "hoppy");
    }
}
