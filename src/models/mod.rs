//! The `Model` module defines a container for data, and also interfaces for
//! syncing said data to local databases.

#[macro_use]
pub mod protected;
pub mod user;

use ::serde::ser::Serialize;
use ::serde::de::Deserialize;

use ::error::{TError, TResult};
use ::util::json;
use ::util::event::Emitter;

/// A macro that makes it easier to define a getter/setter intermediary datatype
/// (ModelData[Ref]) for our Model trait
macro_rules! make_macro_data {
    (
        $( $name:ident($datatype:ty), )*
    ) => {
        pub enum ModelData {
            $( $name($datatype), )*
        }

        pub enum ModelDataRef<'a> {
            $( $name(&'a $datatype), )*
        }
    }
}

make_macro_data! {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Bin(Vec<u8>),
}

/// A macro to make it easy to create From impls for ModelData
macro_rules! make_model_from {
    ($field:ident, $t:ty) => (
        impl<'a> From<&'a $t> for ModelDataRef<'a> {
            fn from(val: &'a $t) -> ModelDataRef<'a> {
                ModelDataRef::$field(val)
            }
        }

        impl<'a> From<ModelDataRef<'a>> for TResult<&'a $t> {
            fn from(val: ModelDataRef<'a>) -> TResult<&'a $t> {
                match val {
                    ModelDataRef::$field(x) => Ok(x),
                    _ => Err(TError::BadValue(format!("ModelDataRef::from({}) -- error converting to raw type", stringify!($t)))),
                }
            }
        }

        impl From<$t> for ModelData {
            fn from(val: $t) -> ModelData {
                ModelData::$field(val)
            }
        }

        impl From<ModelData> for $t {
            fn from(val: ModelData) -> $t {
                match val {
                    ModelData::$field(x) => x,
                    // should NEVER get here, right?
                    _ => panic!("ModelData failure"),
                }
            }
        }
    )
}

make_model_from!(Bool, bool);
make_model_from!(I64, i64);
make_model_from!(U64, u64);
make_model_from!(F64, f64);
make_model_from!(String, String);
make_model_from!(Bin, Vec<u8>);

/// The model trait defines an interface for (de)serializable objects that track
/// their changes via eventing.
pub trait Model2: Emitter + Serialize + Deserialize {
    /// Get a field's value out of this model by field name
    fn get<'a, T>(&'a self, field: &str) -> TResult<&'a T>
        where ModelDataRef<'a>: From<&'a T>,
              TResult<&'a T>: From<ModelDataRef<'a>>;

    /// Set a value into a field in this modle by field name
    fn set<T>(&mut self, field: &str, val: T) -> TResult<()>
        where ModelData: From<T>,
              T: From<ModelData> + ::util::json::Serialize;

    /// Get this model's ID
    fn id<'a, T>(&'a self) -> TResult<&'a T>
        where ModelDataRef<'a>: From<&'a T>,
              TResult<&'a T>: From<ModelDataRef<'a>>
    {
        self.get("id")
    }
}

macro_rules! model {
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $( $field:ident: $field_type:ty, )*
        }
    ) => {
        serializable! {
            $(#[$struct_meta])*
            pub struct $name {
                ( $( $unserialized: $unserialized_type, )*
                  _emitter: ::util::event::EventEmitter )
                id: String,
                $( $field: $field_type, )*
            }
        }

        impl $name {
            fn new() -> $name {
                $name {
                    id: Default::default(),
                    $(
                        $field: Default::default(),
                    )*
                    _emitter: ::util::event::EventEmitter::new(),
                }
            }
        }

        impl Emitter for $name {
            fn bindings(&mut self) -> &mut ::util::event::Bindings {
                self._emitter.bindings()
            }
        }

        impl Model2 for $name {
            fn get<'a, T>(&'a self, field: &str) -> TResult<&'a T>
                where ModelDataRef<'a>: From<&'a T>,
                      TResult<&'a T>: From<ModelDataRef<'a>>
            {
                let val: ModelDataRef;
                if field == "id" {
                    val = From::from(&self.id);
                    return From::from(val);
                }
                $(
                    if field == stringify!($field) {
                        val = From::from(&self.$field);
                        return From::from(val);
                    }
                )*
                Err(TError::MissingField(format!("Model::get() -- missing field `{}`", field)))
            }

            fn set<T>(&mut self, field: &str, val: T) -> TResult<()>
                where ModelData: From<T>,
                      T: From<ModelData> + ::util::json::Serialize
            {
                let data: ModelData = From::from(val);
                if field == "id" {
                    self.id = From::from(data);
                    self.trigger("set:id", &::util::json::Value::Null);
                    return Ok(())
                }
                $(
                    if field == stringify!($field) {
                        self.$field = From::from(data);
                        self.trigger(concat!("set:", stringify!($field)), &::util::json::Value::Null);
                        return Ok(())
                    }
                )*
                Err(TError::MissingField(format!("Model::get() -- missing field `{}`", field)))
            }
        }
    }
}

pub trait Model: Emitter {
    /// Grab *all* data for this model
    fn data(&self) -> &json::Value;

    /// Grab *all* data for this model as mutable
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

    /// Get a value from this model's data, but if it's None, return a TError
    /// MissingField error.
    fn get_err<T>(&self, field: &str) -> TResult<T>
        where T: json::Serialize + json::Deserialize
    {
        match self.get_nest(&[field]) {
            Some(x) => Ok(x),
            None => Err(TError::MissingField(String::from(field))),
        }
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
    use ::error::{TResult, TError};
    use ::util::event::{self, Emitter};
    use std::sync::{Arc, RwLock};

    model! {
        pub struct Rabbit {
            ()
            name: String,
            city: String,
            chews_on_things_that_dont_belong_to_him: bool,
        }
    }

    #[test]
    fn getter_setter() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.name, "");
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, false);
        assert_eq!(rabbit.get::<String>("name").unwrap(), "");
        assert_eq!(rabbit.get::<bool>("chews_on_things_that_dont_belong_to_him").unwrap(), &false);

        rabbit.set("name", String::from("Shredder")).unwrap();
        rabbit.set("chews_on_things_that_dont_belong_to_him", true).unwrap();

        assert_eq!(rabbit.name, "Shredder");
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, true);
        assert_eq!(rabbit.get::<String>("name").unwrap(), "Shredder");
        assert_eq!(rabbit.get::<bool>("chews_on_things_that_dont_belong_to_him").unwrap(), &true);

        match rabbit.set("rhymes_with_heinous", 69i64) {
            Ok(_) => panic!("whoa, whoa, whoa. set a non-existent field"),
            Err(_) => {},
        }
    }

    #[test]
    fn get_id() {
        let mut rabbit = Rabbit::new();
        rabbit.set("id", String::from("696969")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "696969");
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
            let mut rabbit = Bunny::new();
            rabbit.bind("hop", cb, "rabbit:hop");

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
        panic!("build some event tests!");
    }

    // ----------- model v1 -----------------



    struct Bunny {
        _data: Value,
        emitter: ::util::event::EventEmitter,
    }

    impl Bunny {
        fn new() -> Bunny {
            Bunny {
                _data: json::obj(),
                emitter: event::EventEmitter::new(),
            }
        }
    }

    impl Emitter for Bunny {
        fn bindings(&mut self) -> &mut ::util::event::Bindings {
            self.emitter.bindings()
        }
    }

    impl Model for Bunny {
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
    fn _get_set() {
        let mut rabbit = Bunny::new();
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
    fn _ids() {
        let mut rabbit = Bunny::new();
        rabbit.set("id", &String::from("696969")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "696969");
    }

    #[test]
    fn _clears() {
        let mut rabbit = Bunny::new();
        rabbit.set("id", &String::from("omglol")).unwrap();
        rabbit.set("name", &String::from("hoppy")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "omglol");
        assert_eq!(rabbit.get::<String>("name").unwrap(), "hoppy");
        rabbit.clear();
        assert_eq!(rabbit.id::<String>(), None);
        assert_eq!(rabbit.get::<String>("name"), None);
    }

    #[test]
    fn _resets() {
        let mut rabbit1 = Bunny::new();
        let mut rabbit2 = Bunny::new();
        rabbit1.set("id", &String::from("omglol")).unwrap();
        rabbit1.set("name", &String::from("hoppy")).unwrap();
        rabbit2.reset(rabbit1.data().clone()).unwrap();
        assert_eq!(rabbit2.id::<String>().unwrap(), "omglol");
        assert_eq!(rabbit2.get::<String>("name").unwrap(), "hoppy");
    }

    #[test]
    fn _set_multi() {
        let mut rabbit = Bunny::new();
        rabbit.set("name", &String::from("flirty")).unwrap();
        assert_eq!(rabbit.get::<String>("name").unwrap(), "flirty");
        assert_eq!(rabbit.get::<i64>("age"), None);
        let json = json::parse(&String::from(r#"{"name":"vozzie","age":3}"#)).unwrap();
        rabbit.set_multi(json).unwrap();
        assert_eq!(rabbit.get::<String>("name").unwrap(), "vozzie");
        assert_eq!(rabbit.get::<i64>("age").unwrap(), 3);
    }

    #[test]
    fn _bind_trigger() {
        let data = Arc::new(RwLock::new(vec![0]));
        let rdata = data.clone();
        {
            let data = data.clone();
            let cb = move |_: &Value| {
                data.write().unwrap()[0] += 1;
            };
            let mut rabbit = Bunny::new();
            rabbit.bind("hop", cb, "rabbit:hop");

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
    fn _built_in_events() {
        let map: HashMap<String, i64> = HashMap::new();
        let data = Arc::new(RwLock::new(map));
        let rdata = data.clone();
        {
            let data1 = data.clone();
            let data2 = data.clone();

            let mut rabbit = Bunny::new();
            rabbit.bind("set:name", move |val: &Value| {
                let string = match *val {
                    json::Value::String(ref x) => x.clone(),
                    _ => panic!("got non-string type"),
                };
                let mut hash = data1.write().unwrap();
                let count = match hash.get(&string) {
                    Some(x) => x.clone(),
                    None => 0i64,
                };
                hash.insert(string, count + 1);
            }, "setters");
            rabbit.bind("set:second_name", move |val: &Value| {
                let string = match *val {
                    json::Value::String(ref x) => x.clone(),
                    _ => panic!("got non-string type"),
                };
                let mut hash = data2.write().unwrap();
                let count = match hash.get(&string) {
                    Some(x) => x.clone(),
                    None => 0i64,
                };
                hash.insert(string, count + 1);
            }, "setters");
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

