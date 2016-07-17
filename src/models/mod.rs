//! The `Model` module defines a container for data, and also interfaces for
//! syncing said data to local databases.
//!
//! TODO: events for clear/reset
//! TODO: reset should use clear/set_multi

#[macro_use]
pub mod protected;
pub mod user;
pub mod file;
pub mod note;

use ::error::{TError, TResult};
use ::util::json;
use ::util::event::Emitter;

pub trait Model<'event>: Emitter<'event> {
    /// Grab *all* data for this model (safe or not)
    fn data(&self) -> &json::Value;

    /// Grab *all* data for this model (safe or not) as mutable
    fn data_mut(&mut self) -> &mut json::Value;

    /// Clear out the data in this Model
    fn clear(&mut self) -> () {
        match self.data_mut() {
            &mut json::Value::Object(ref mut x) => { x.clear(); },
            _ => {},
        }
        self.trigger("clear", &json::obj());
    }

    /// Reset the data in this Model with a new Value
    fn reset(&mut self, data: json::Value) -> TResult<()> {
        self.clear();
        try!(self.set_multi(data));
        self.trigger("reset", &json::obj());
        Ok(())
    }

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
        let res = try!(self.set_nest(&[field], value));
        self.trigger(&format!("set:{}", field)[..], &json::to_val(&value));
        Ok(res)
    }

    /// Like set(), but instead of setting a T value into a key in the model, we
    /// pass a set of json Value data and set each item in there into the
    /// model's data.
    fn set_multi(&mut self, data: json::Value) -> TResult<()> {
        let hash = match data {
            json::Value::Object(x) => x,
            _ => return Err(TError::BadValue(format!("Model::set_multi() - `data` value given was not an object type"))),
        };
        let mut events: Vec<(String, json::Value)> = Vec::new();
        {
            let self_data = match self.data_mut() {
                &mut json::Value::Object(ref mut x) => x,
                _ => return Err(TError::BadValue(format!("Model::set_multi() - self.data_mut() returned non-object type"))),
            };
            for (key, val) in hash {
                let val_clone = val.clone();
                self_data.insert(key.clone(), val);
                events.push((key, val_clone));
            }
        }
        for (key, val) in events {
            self.trigger(&format!("set:{}", key)[..], &val);
        }
        Ok(())
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
    use ::util::json::{self, Value};
    use ::std::collections::HashMap;
    use ::error::{TError};
    use ::util::event::{self, Emitter};
    use std::sync::{Arc, RwLock};

    struct Rabbit<'event> {
        _data: Value,
        emitter: ::util::event::EventEmitter<'event>,
    }

    impl<'event> Rabbit<'event> {
        fn new() -> Rabbit<'event> {
            Rabbit {
                _data: json::obj(),
                emitter: event::EventEmitter::new(),
            }
        }
    }

    impl<'event> Emitter<'event> for Rabbit<'event> {
        fn bindings(&mut self) -> &mut ::std::collections::HashMap<&'event str, Vec<::util::event::Callback<'event>>> {
            self.emitter.bindings()
        }
    }

    impl<'event> Model<'event> for Rabbit<'event> {
        fn data(&self) -> &Value {
            &self._data
        }

        fn data_mut(&mut self) -> &mut Value {
            &mut self._data
        }

        fn clear(&mut self) -> () {
            self._data = json::obj();
        }

        fn reset(&mut self, data: Value) -> ::error::TResult<()> {
            match data {
                Value::Object(..) => {
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

    #[test]
    fn set_multi() {
        let mut rabbit = Rabbit::new();
        rabbit.set("name", &String::from("flirty")).unwrap();
        assert_eq!(rabbit.get::<String>("name").unwrap(), "flirty");
        assert_eq!(rabbit.get::<i64>("age"), None);
        let json = json::parse(&String::from(r#"{"name":"vozzie","age":3}"#)).unwrap();
        rabbit.set_multi(json).unwrap();
        assert_eq!(rabbit.get::<String>("name").unwrap(), "vozzie");
        assert_eq!(rabbit.get::<i64>("age").unwrap(), 3);
    }

    #[test]
    fn bind_trigger() {
        let data = Arc::new(RwLock::new(vec![0]));
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |_: &Value| {
                data.write().unwrap()[0] += 1;
            };
            let mut rabbit = Rabbit::new();
            rabbit.bind("hop", &cb, "rabbit:hop");

            let jval = json::obj();
            assert_eq!(rdata.read().unwrap()[0], 0);
            rabbit.trigger("hellp", &jval);
            assert_eq!(rdata.read().unwrap()[0], 0);
            rabbit.trigger("hop", &jval);
            assert_eq!(rdata.read().unwrap()[0], 1);
            rabbit.trigger("hop", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);

            rabbit.unbind("hop", "rabbit:hop");

            rabbit.trigger("hop", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);
            rabbit.trigger("hop", &jval);
            assert_eq!(rdata.read().unwrap()[0], 2);
        }
    }

    #[test]
    fn built_in_events() {
        let map: HashMap<String, i64> = HashMap::new();
        let data = Arc::new(RwLock::new(map));
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |val: &Value| {
                let string = match *val {
                    json::Value::String(ref x) => x.clone(),
                    _ => panic!("got non-string type"),
                };
                let mut hash = data.write().unwrap();
                let count = match hash.get(&string) {
                    Some(x) => x.clone(),
                    None => 0i64,
                };
                hash.insert(string, count + 1);
            };

            let mut rabbit = Rabbit::new();
            rabbit.bind("set:name", &cb, "setters");
            rabbit.bind("set:second_name", &cb, "setters");
            let hash = rdata.read().unwrap();
            assert_eq!(hash.get(&String::from("blackberry")), None);
            assert_eq!(hash.get(&String::from("murdery")), None);
            drop(hash);
            rabbit.set("name", &String::from("blackberry")).unwrap();
            rabbit.set("second_name", &String::from("murdery")).unwrap();
            let hash = rdata.read().unwrap();
            assert_eq!(hash.get(&String::from("blackberry")), Some(&1));
            assert_eq!(hash.get(&String::from("murdery")), Some(&1));
        }
    }
}

