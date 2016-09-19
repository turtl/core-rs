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

        impl<'a> From<Option<&'a mut $t>> for ModelDataRefMut<'a> {
            fn from(val: Option<&'a mut $t>) -> ModelDataRefMut<'a> {
                ModelDataRefMut::$field(val)
            }
        }

        impl<'a> From<ModelDataRefMut<'a>> for Option<&'a mut $t> {
            fn from(val: ModelDataRefMut<'a>) -> Option<&'a mut $t> {
                match val {
                    ModelDataRefMut::$field(x) => x,
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
        #[derive(Debug)]
        pub enum ModelData {
            $( $name(Option<$datatype>), )*
        }

        #[derive(Debug)]
        pub enum ModelDataRef<'a> {
            $( $name(Option<&'a $datatype>), )*
        }

        #[derive(Debug)]
        pub enum ModelDataRefMut<'a> {
            $( $name(Option<&'a mut $datatype>), )*
        }

        $(
            make_model_from!($name, $datatype);
        )*
    }
}

impl From<Value> for ModelData {
    fn from(val: Value) -> ModelData {
        let blankval = ModelData::Bool(None);
        match val {
            Value::Null => blankval,
            Value::Bool(x) => ModelData::Bool(Some(x)),
            Value::I64(x) => ModelData::I64(Some(x)),
            Value::F64(x) => ModelData::F64(Some(x)),
            Value::String(x) => ModelData::String(Some(x)),
            Value::Array(_) => {
                // this one's weird. we're going to just assume that our array
                // is a Vec<String>. we *could* do Vec<u8> except we're never
                // going to pass binary data via a JSON array (we use base64) so
                // the only other Vec type we have is Vec<String> (like tags).
                let arr: Vec<String> = match json::from_val(val) {
                    Ok(x) => x,
                    Err(_) => {
                        warn!("ModelData::from(Value) -- problem decoding Array type (couldn't find matching type)");
                        return blankval;
                    },
                };
                ModelData::List(Some(arr))
            },
            _ => blankval,
        }
    }
}

make_macro_data! {
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Bin(Vec<u8>),
    List(Vec<String>),
}

/// The model trait defines an interface for (de)serializable objects that track
/// their changes via eventing.
pub trait Model: Emitter + Serialize + Deserialize {
    /// Get the fields in this model
    fn fields(&self) -> Vec<&'static str>;

    /// Get the raw ModelDataRef object for a field
    fn get_raw<'a>(&'a self, field: &str) -> ModelDataRef<'a>;

    /// Get the raw ModelDataRefMut object for a field
    fn get_raw_mut<'a>(&'a mut self, field: &str) -> ModelDataRefMut<'a>;

    /// Get a field's value out of this model by field name
    fn get<'a, T>(&'a self, field: &str) -> Option<&'a T>
        where ModelDataRef<'a>: From<Option<&'a T>>,
              Option<&'a T>: From<ModelDataRef<'a>>
    {
        From::from(self.get_raw(field))
    }

    /// Get a field's value out of this model by field name
    fn get_mut<'a, T>(&'a mut self, field: &str) -> Option<&'a mut T>
        where ModelDataRefMut<'a>: From<Option<&'a mut T>>,
              Option<&'a mut T>: From<ModelDataRefMut<'a>>
    {
        From::from(self.get_raw_mut(field))
    }

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

    /// Is this model new?
    fn is_new(&self) -> bool {
        self.get::<String>("id").is_none()
    }

    /// Clear out this model's data
    fn clear(&mut self) -> TResult<()>;

    /// Set multiple values into this model
    fn set_multi(&mut self, data: Value) -> TResult<()>;

    /// Clear out a model field
    fn unset(&mut self, field: &str) -> TResult<()>;

    /// Reset a model with a JSON Value *object*
    fn reset(&mut self, data: Value) -> TResult<()> {
        try!(self.clear());
        let res = self.set_multi(data);
        self.trigger("reset", &::util::json::Value::Null);
        res
    }

    /// Turn this model into a JSON string
    fn stringify(&self) -> TResult<String> {
        json::stringify(self).map_err(|e| toterr!(e))
    }
}

#[macro_export]
/// Defines a model given a set of serializable fields, and also fields that
/// exist under the model that are NOT meant to be serialized.
macro_rules! model {
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ($( $unserialized:ident: $unserialized_type:ty ),*)
            $( $field:ident: $field_type:ty, )*
        }
    ) => {
        #[allow(dead_code)]
        use ::util::event::Emitter as PEmitter;

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
            pub fn new() -> $name {
                $name {
                    id: None,
                    $( $field: None, )*
                    $( $unserialized: Default::default(), )*
                    _emitter: ::util::event::EventEmitter::new(),
                }
            }
        }

        impl ::util::event::Emitter for $name {
            fn bindings(&mut self) -> &mut ::util::event::Bindings {
                self._emitter.bindings()
            }
        }

        impl ::models::model::Model for $name {
            fn fields(&self) -> Vec<&'static str> {
                vec![ $( stringify!($field) ),* ]
            }

            fn get_raw<'a>(&'a self, field: &str) -> ::models::model::ModelDataRef<'a> {
                let val: ::models::model::ModelDataRef;
                if field == "id" {
                    val = From::from(match self.id {
                        Some(ref x) => Some(x),
                        None => None,
                    });
                    return val;
                }
                $(
                    if field == fix_type!(stringify!($field)) {
                        val = From::from(match self.$field {
                            Some(ref x) => Some(x),
                            None => None,
                        });
                        return val;
                    }
                )*
                ::models::model::ModelDataRef::Bool(None)
            }

            fn get_raw_mut<'a>(&'a mut self, field: &str) -> ::models::model::ModelDataRefMut<'a> {
                let val: ::models::model::ModelDataRefMut;
                if field == "id" {
                    val = From::from(match self.id {
                        Some(ref mut x) => Some(x),
                        None => None,
                    });
                    return val;
                }
                $(
                    if field == fix_type!(stringify!($field)) {
                        val = From::from(match self.$field {
                            Some(ref mut x) => Some(x),
                            None => None,
                        });
                        return val;
                    }
                )*
                ::models::model::ModelDataRefMut::Bool(None)
            }

            fn unset(&mut self, field: &str) -> ::error::TResult<()> {
                if field == "id" {
                    self.id = None;
                }
                $(
                    if field == fix_type!(stringify!($field)) {
                        self.$field = None;
                    }
                )*
                Ok(())
            }

            fn set_raw<T>(&mut self, field: &str, val: Option<T>) -> ::error::TResult<()>
                where ::models::model::ModelData: From<Option<T>>,
                      Option<T>: From<::models::model::ModelData> + ::util::json::Serialize
            {
                let data: ::models::model::ModelData = From::from(val);
                if field == "id" {
                    self.id = From::from(data);
                    self.trigger("set:id", &::util::json::Value::Null);
                    return Ok(())
                }
                $(
                    if field == fix_type!(stringify!($field)) {
                        self.$field = From::from(data);
                        self.trigger(concat!("set:", stringify!($field)), &::util::json::Value::Null);
                        return Ok(())
                    }
                )*
                Err(::error::TError::MissingField(format!("Model::get() -- missing field `{}`", field)))
            }

            fn clear(&mut self) -> ::error::TResult<()> {
                $(
                    try!(self.set_raw::<$field_type>(fix_type!(stringify!($field)), None));
                )*
                self.trigger("clear", &::util::json::Value::Null);
                Ok(())
            }

            fn set_multi(&mut self, data: ::util::json::Value) -> ::error::TResult<()> {
                let mut hash = match data {
                    ::util::json::Value::Object(x) => x,
                    _ => return Err(::error::TError::BadValue(format!("Model::reset() -- data given was not a JSON object"))),
                };

                $({
                    let field_str = fix_type!(stringify!($field));
                    let field = String::from(field_str);
                    if hash.contains_key(&field) {
                        let modeldata: ::models::model::ModelData = From::from(hash.remove(&field).unwrap());
                        let val: Option<$field_type> = From::from(modeldata);
                        try!(self.set_raw(field_str, val));
                    }
                })*
                Ok(())
            }
        }

        impl ::std::default::Default for $name {
            fn default() -> Self {
                $name::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json::{self, Value};
    use std::sync::{Arc, RwLock};

    model! {
        pub struct Rabbit {
            ()
            name: String,
            type_: String,
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
            }, "clearrr");
            let data4 = data.clone();
            rabbit.bind("reset", move |_| {
                data4.write().unwrap()[3] += 1;
            }, "resetttttlol");

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
            rabbit.set_multi(json).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 4);
            assert_eq!(rdata.read().unwrap()[1], 3);
            assert_eq!(rdata.read().unwrap()[2], 1);
            assert_eq!(rdata.read().unwrap()[3], 0);

            let json: Value = json::parse(&String::from(r#"{"name":"slappy"}"#)).unwrap();
            rabbit.reset(json).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 6);
            assert_eq!(rdata.read().unwrap()[1], 4);
            assert_eq!(rdata.read().unwrap()[2], 2);
            assert_eq!(rdata.read().unwrap()[3], 1);
        }
    }

    #[test]
    fn stringify() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.stringify().unwrap(), "{\"id\":null,\"name\":null,\"type\":null,\"city\":null,\"chews_on_things_that_dont_belong_to_him\":null}");

        rabbit.set("id", String::from("12345")).unwrap();
        rabbit.type_ = Some(String::from("hopper"));
        rabbit.city = Some(String::from("sc"));

        assert_eq!(rabbit.stringify().unwrap(), "{\"id\":\"12345\",\"name\":null,\"type\":\"hopper\",\"city\":\"sc\",\"chews_on_things_that_dont_belong_to_him\":null}");
    }

    #[test]
    fn is_new() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.is_new(), true);
        rabbit.set("id", String::from("6969")).unwrap();
        assert_eq!(rabbit.is_new(), false);
    }
}


