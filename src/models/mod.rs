//! The `Model` module defines a container for data, and also interfaces for
//! syncing said data to local databases.

#[macro_use]
pub mod protected;
pub mod user;

use ::serde::ser::Serialize;
use ::serde::de::Deserialize;

use ::error::{TError, TResult};
use ::util::json::{self, Value};
use ::util::event::Emitter;

/// A macro to make it easy to create From impls for ModelData
macro_rules! make_model_from {
    ($field:ident, $t:ty) => (
        impl<'a> From<Option<&'a $t>> for ModelDataRef<'a> {
            fn from(val: Option<&'a $t>) -> ModelDataRef<'a> {
                ModelDataRef::$field(val)
            }
        }

        impl<'a> From<ModelDataRef<'a>> for Option<&'a $t> {
            fn from(val: ModelDataRef<'a>) -> Option<&'a $t> {
                match val {
                    ModelDataRef::$field(x) => x,
                    _ => None,
                }
            }
        }

        impl From<Option<$t>> for ModelData {
            fn from(val: Option<$t>) -> ModelData {
                ModelData::$field(val)
            }
        }

        impl From<ModelData> for Option<$t> {
            fn from(val: ModelData) -> Option<$t> {
                match val {
                    ModelData::$field(x) => x,
                    _ => None,
                }
            }
        }
    )
}

/// A macro that makes it easier to define a getter/setter intermediary datatype
/// (ModelData[Ref]) for our Model trait
macro_rules! make_macro_data {
    (
        $( $name:ident($datatype:ty), )*
    ) => {
        pub enum ModelData {
            $( $name(Option<$datatype>), )*
        }

        pub enum ModelDataRef<'a> {
            $( $name(Option<&'a $datatype>), )*
        }

        $(
            make_model_from!($name, $datatype);
        )*
    }
}

impl From<Value> for ModelData {
    fn from(val: Value) -> ModelData {
        match val {
            Value::Null => ModelData::Bool(None),
            Value::Bool(x) => ModelData::Bool(Some(x)),
            Value::I64(x) => ModelData::I64(Some(x)),
            Value::U64(x) => ModelData::U64(Some(x)),
            Value::F64(x) => ModelData::F64(Some(x)),
            Value::String(x) => ModelData::String(Some(x)),
            _ => ModelData::Bool(None),
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

/// The model trait defines an interface for (de)serializable objects that track
/// their changes via eventing.
pub trait Model2: Emitter + Serialize + Deserialize {
    /// Get the fields in this model
    fn fields(&self) -> Vec<&'static str>;

    /// Get a field's value out of this model by field name
    fn get<'a, T>(&'a self, field: &str) -> Option<&'a T>
        where ModelDataRef<'a>: From<Option<&'a T>>,
              Option<&'a T>: From<ModelDataRef<'a>>;

    /// Set an Option value into a field in this model by field name
    fn set_raw<T>(&mut self, field: &str, val: Option<T>) -> TResult<()>
        where ModelData: From<Option<T>>,
              Option<T>: From<ModelData> + ::util::json::Serialize;

    /// Set a value into a field in this model by field name
    fn set<T>(&mut self, field: &str, val: T) -> TResult<()>
        where ModelData: From<Option<T>>,
              Option<T>: From<ModelData> + ::util::json::Serialize
    {
        self.set_raw(field, Some(val))
    }

    /// Get this model's ID
    fn id<'a, T>(&'a self) -> Option<&'a T>
        where ModelDataRef<'a>: From<Option<&'a T>>,
              Option<&'a T>: From<ModelDataRef<'a>>
    {
        self.get("id")
    }

    /// Clear out this model's data
    fn clear(&mut self) -> TResult<()>;

    /// Reset a model with a JSON Value *object*
    fn reset(&mut self, data: Value) -> TResult<()>;
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
                id: Option<String>,
                $( $field: Option<$field_type>, )*
            }
        }

        impl $name {
            fn new() -> $name {
                $name {
                    id: None,
                    $( $field: None, )*
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
            fn fields(&self) -> Vec<&'static str> {
                vec![ $( stringify!($field) ),* ]
            }

            fn get<'a, T>(&'a self, field: &str) -> Option<&'a T>
                where ModelDataRef<'a>: From<Option<&'a T>>,
                      Option<&'a T>: From<ModelDataRef<'a>>
            {
                let val: ModelDataRef;
                if field == "id" {
                    val = From::from(match self.id {
                        Some(ref x) => Some(x),
                        None => None,
                    });
                    return From::from(val);
                }
                $(
                    if field == stringify!($field) {
                        val = From::from(match self.$field {
                            Some(ref x) => Some(x),
                            None => None,
                        });
                        return From::from(val);
                    }
                )*
                None
            }

            fn set_raw<T>(&mut self, field: &str, val: Option<T>) -> TResult<()>
                where ModelData: From<Option<T>>,
                      Option<T>: From<ModelData> + ::util::json::Serialize
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

            fn clear(&mut self) -> TResult<()> {
                $(
                    try!(self.set_raw::<$field_type>(stringify!($field), None));
                )*
                self.trigger("clear", &::util::json::Value::Null);
                Ok(())
            }

            fn reset(&mut self, data: Value) -> TResult<()> {
                let mut hash = match data {
                    ::util::json::Value::Object(x) => x,
                    _ => return Err(TError::BadValue(format!("Model::reset() -- data given was not a JSON object"))),
                };

                $({
                    let field = String::from(stringify!($field));
                    if hash.contains_key(&field) {
                        let modeldata: ModelData = From::from(hash.remove(&field).unwrap());
                        let val: Option<$field_type> = From::from(modeldata);
                        try!(self.set_raw(stringify!($field), val));
                    }
                })*
                self.trigger("reset", &::util::json::Value::Null);
                Ok(())
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
        assert_eq!(rabbit.name, None);
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, None);
        assert_eq!(rabbit.get::<String>("name"), None);
        assert_eq!(rabbit.get::<bool>("chews_on_things_that_dont_belong_to_him"), None);

        rabbit.set("name", String::from("Shredder")).unwrap();
        rabbit.set_raw("chews_on_things_that_dont_belong_to_him", Some(true)).unwrap();

        assert_eq!(rabbit.name, Some(String::from("Shredder")));
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, Some(true));
        assert_eq!(rabbit.get::<String>("name"), Some(&String::from("Shredder")));
        assert_eq!(rabbit.get::<bool>("chews_on_things_that_dont_belong_to_him"), Some(&true));

        match rabbit.set("rhymes_with_heinous", 69i64) {
            Ok(_) => panic!("whoa, whoa, whoa. set a non-existent field"),
            Err(_) => {},
        }

        assert_eq!(rabbit.get::<f64>("get_a_job"), None);
    }

    #[test]
    fn get_id() {
        let mut rabbit = Rabbit::new();
        rabbit.set("id", String::from("696969")).unwrap();
        assert_eq!(rabbit.id::<String>().unwrap(), "696969");
    }

    #[test]
    fn reset_clear() {
        let mut rabbit = Rabbit::new();

        rabbit.set("name", String::from("hoppy")).unwrap();
        rabbit.set("city", String::from("santa cruz")).unwrap();

        rabbit.clear().unwrap();

        assert_eq!(rabbit.name, None);
        assert_eq!(rabbit.city, None);

        let json: Value = json::parse(&String::from(r#"{"has_job":false,"name":"slappy","city":"duluth"}"#)).unwrap();
        rabbit.reset(json).unwrap();

        assert_eq!(rabbit.name, Some(String::from("slappy")));
        assert_eq!(rabbit.city, Some(String::from("duluth")));
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
        let data = Arc::new(RwLock::new(vec![0, 0, 0, 0]));
        let rdata = data.clone();
        {
            let data = data.clone();
            let mut rabbit = Rabbit::new();

            let data1 = data.clone();
            rabbit.bind("set:name", move |_| {
                data1.write().unwrap()[0] += 1;
            }, "naem set");
            let data2 = data.clone();
            rabbit.bind("set:city", move |_| {
                data2.write().unwrap()[1] += 1;
            }, "ciyt set");
            let data3 = data.clone();
            rabbit.bind("clear", move |_| {
                data3.write().unwrap()[2] += 1;
            }, "ciyt set");
            let data4 = data.clone();
            rabbit.bind("reset", move |_| {
                data4.write().unwrap()[3] += 1;
            }, "ciyt set");

            assert_eq!(rdata.read().unwrap()[0], 0);
            assert_eq!(rdata.read().unwrap()[1], 0);
            assert_eq!(rdata.read().unwrap()[2], 0);
            assert_eq!(rdata.read().unwrap()[3], 0);
            rabbit.set("name", String::from("bernard")).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 1);
            assert_eq!(rdata.read().unwrap()[1], 0);
            assert_eq!(rdata.read().unwrap()[2], 0);
            assert_eq!(rdata.read().unwrap()[3], 0);
            rabbit.set("name", String::from("gertrude")).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 2);
            assert_eq!(rdata.read().unwrap()[1], 0);
            assert_eq!(rdata.read().unwrap()[2], 0);
            assert_eq!(rdata.read().unwrap()[3], 0);
            rabbit.set("city", String::from("san franciscy")).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 2);
            assert_eq!(rdata.read().unwrap()[1], 1);
            assert_eq!(rdata.read().unwrap()[2], 0);
            assert_eq!(rdata.read().unwrap()[3], 0);
            rabbit.set("city", String::from("santa cruz")).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 2);
            assert_eq!(rdata.read().unwrap()[1], 2);
            assert_eq!(rdata.read().unwrap()[2], 0);
            assert_eq!(rdata.read().unwrap()[3], 0);

            rabbit.clear().unwrap();
            assert_eq!(rdata.read().unwrap()[0], 3);
            assert_eq!(rdata.read().unwrap()[1], 3);
            assert_eq!(rdata.read().unwrap()[2], 1);
            assert_eq!(rdata.read().unwrap()[3], 0);

            let json: Value = json::parse(&String::from(r#"{"name":"slappy"}"#)).unwrap();
            rabbit.reset(json).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 4);
            assert_eq!(rdata.read().unwrap()[1], 3);
            assert_eq!(rdata.read().unwrap()[2], 1);
            assert_eq!(rdata.read().unwrap()[3], 1);
        }
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

