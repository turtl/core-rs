use std::collections::BTreeMap;

use ::std::fmt;

use ::error::{TResult, TError};
use ::models::Model;
use ::util::json::{self};

/// The Protected trait defines a set of functionality for our models such that
/// they are able to be properly (de)serialized (including encryption/decryption
/// of the model).
///
/// It also defines methods that make it easy to do The Right Thing (c)(r)(tm)
/// when handling protected model data. The goal here is to eliminate all forms
/// of data leaks while providing an interface that's easy to use.
pub trait Protected: Model + fmt::Debug {
    /// Grab the public fields for this model
    fn public_fields(&self) -> Vec<&str>;

    /// Grab the private fields for this model
    fn private_fields(&self) -> Vec<&str>;

    /// Grab all public fields for this model as a json Value
    fn safe_data(&self) -> json::Value {
        let mut map: BTreeMap<String, json::Value> = BTreeMap::new();
        let data = self.data();
        for field in &self.public_fields() {
            let val = json::walk(&[field], &data);
            match val {
                Ok(v) => { map.insert(String::from(*field), v.clone()); },
                Err(..) => {}
            }
        }
        json::Value::Object(map)
    }

    /// Return a JSON dump of all public fields. Really, this is a wrapper
    /// around `json::stringify(model.safe_data())`
    fn safe_stringify(&self) -> TResult<String> {
        let safe = self.safe_data();
        json::stringify(&safe).map_err(|e| toterr!(e))
    }
}

/// Given a &str value, checks to see if it matches "type_", and if so returns
/// "type" instead. It also does the reverse: if it detects "type", it returns
/// "type_". That way we can use this
///
/// This is useful for translating between rust structs, which don't allow a
/// field named `type` and our JSON objects out in the wild, many of which *do*
/// have a `type` field.
#[macro_export]
macro_rules! fix_type {
    ( "type" ) => { "type_" };
    ( "type_" ) => { "type" };
    ( $val:expr ) => {
        {
            let myval = $val;
            match myval {
                "type_" => "type",
                "type" => "type_",
                _ => myval
            }
        }
    }
}

/// Defines a protected model for us. We give it a model name, a set of public
/// fields, a set of private fields, and lastly a set of extra fields (neither
/// public nor private) and it defines our model struct, and implements the
/// Protected trait for us, as well as a handy debug trait (that won't leak
/// private information on print).
///
/// NOTE that the `id` and `body` fields are always prepended to the public
/// field list as `id: String` and `body: String` so don't include the id/body
/// fields in your public/private field lists.
///
/// # Examples
///
/// ```
/// # #[macro_use] mod models;
/// # fn main() {
/// protected!(Squirrel, (size: u64), (name: String), ());
/// # }
/// ```
#[macro_export]
macro_rules! protected {
    // pub
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ( $( $pub_field:ident: $pub_type:ty ),* ),
            ( $( $priv_field:ident: $priv_type:ty ),* ),
            ( $( $extra_field:ident: $extra_type:ty ),* )
        }
    ) => {
        $(#[$struct_meta])*
        pub struct $name {
            $( $extra_field: $extra_type, )*
            _data: ::util::json::Value,
        }

        protected!([IMPL ( $name ), ( $( $pub_field ),* ), ( $( $priv_field ),* ), ( $( $extra_field ),* )]);
    };

    // no pub
    (
        $(#[$struct_meta:meta])*
        struct $name:ident {
            ( $( $pub_field:ident: $pub_type:ty ),* ),
            ( $( $priv_field:ident: $priv_type:ty ),* ),
            ( $( $extra_field:ident: $extra_type:ty ),* )
        }
    ) => {
        $(#[$struct_meta])*
        struct $name {
            $( $extra_field: $extra_type, )*
            _data: ::util::json::Value,
        }

        protected!([IMPL ( $name ), ( $( $pub_field ),* ), ( $( $priv_field ),* ), ( $( $extra_field ),* )]);
    };

    // implementation
    (
        [IMPL ( $name:ident ),
              ( $( $pub_field:ident ),* ),
              ( $( $priv_field:ident ),* ),
              ( $( $extra_field:ident ),* )]

    ) => {
        use ::models::Model as PModel;

        impl $name {
            /// Create an instance of this model, with all values set to None
            #[allow(dead_code)]
            pub fn blank() -> $name {
                $name {
                    _data: ::util::json::obj(),
                    $(
                        $extra_field: Default::default()
                    ),*
                }
            }

            /// Create an instance of this model, given a block of a JSON Value
            #[allow(dead_code)]
            pub fn new(data: ::util::json::Value) -> ::error::TResult<$name> {
                match &data {
                    &::util::json::Value::Object(..) => {},
                    _ => return Err(::error::TError::BadValue(format!("Protected::new(): `data` is not a JSON object"))),
                }
                let mut instance = $name::blank();
                instance._data = data;
                Ok(instance)
            }
        }

        impl PModel for $name {
            fn data(&self) -> &::util::json::Value {
                &self._data
            }

            fn data_mut(&mut self) -> &mut ::util::json::Value {
                &mut self._data
            }

            fn clear(&mut self) -> () {
                self._data = ::util::json::obj();
            }

            fn reset(&mut self, data: ::util::json::Value) -> ::error::TResult<()> {
                match data {
                    ::util::json::Value::Object(..) => {
                        self._data = data;
                        Ok(())
                    }
                    _ => Err(::error::TError::BadValue(String::from("Model::reset(): `data` is not an object type"))),
                }
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                let id = match self.id() {
                    Some(x) => x,
                    None => String::from("<no id>"),
                };
                write!(f, "{}: ({})", stringify!($name), id)
            }
        }

        impl Protected for $name {
            fn public_fields(&self) -> Vec<&str> {
                vec![
                    "id",
                    "body",
                    $( fix_type!(stringify!($pub_field)), )*
                ]
            }

            fn private_fields(&self) -> Vec<&str> {
                vec![
                    $( fix_type!(stringify!($priv_field)), )*
                ]
            }
        }
    }
}

/// Defines a key struct, used by many models that have subkey data.
protected!{
    pub struct Key {
        (), (), ()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json;

    protected!{
        struct Dog {
            ( size: u64 ),
            ( name: String,
              type_: String,
              tags: Vec<String> ),
            ( active: bool )
        }
    }

    #[test]
    fn returns_correct_public_fields() {
        let dog = Dog::blank();
        assert_eq!(dog.public_fields(), ["id", "body", "size"]);
    }

    #[test]
    fn returns_correct_private_fields() {
        let dog = Dog::blank();
        assert_eq!(dog.private_fields(), ["name", "type", "tags"]);
    }

    #[test]
    fn handles_safe_data() {
        let mut dog = Dog::blank();
        dog.active = true;
        dog.set("id", &String::from("123")).unwrap();
        dog.set("size", &42).unwrap();
        dog.set("name", &String::from("barky")).unwrap();
        assert_eq!(json::stringify(&dog.safe_data()).unwrap(), r#"{"id":"123","size":42}"#);
        assert_eq!(dog.safe_stringify().unwrap(), r#"{"id":"123","size":42}"#);
    }

    #[test]
    fn can_serialize_json() {
        /*
        let mut dog = Dog::new();
        dog.size = Some(32);
        dog.name = Some(String::from("timmy"));
        dog.type_ = Some(String::from("tiny"));
        dog.tags = Some(vec![String::from("canine"), String::from("3-legged")]);
        // tests for presence of `extra` fields in JSON (there should be none)
        dog.active = true;
        assert_eq!(json::stringify(&dog).unwrap(), r#"{"id":null,"body":null,"size":32,"name":"timmy","type":"tiny","tags":["canine","3-legged"]}"#);

        dog.tags.as_mut().map(|mut tags| tags.push(String::from("fast")));
        assert_eq!(json::stringify(&dog).unwrap(), r#"{"id":null,"body":null,"size":32,"name":"timmy","type":"tiny","tags":["canine","3-legged","fast"]}"#);
        */
    }
}

