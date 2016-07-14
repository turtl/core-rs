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
    fn untrusted_data(&self) -> json::Value {
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

    /// Return a JSON dump of all fields. Really, this is a wrapper around
    /// `json::stringify(model.data())`.
    ///
    /// Use this function when sending a model to a trusted source (ie inproc
    /// messaging to our view layer).
    ///
    /// __NEVER__ use this function to save data to disk or transmit over a
    /// network connection.
    fn stringify_trusted(&self) -> TResult<String> {
        let safe = self.data();
        json::stringify(&safe).map_err(|e| toterr!(e))
    }

    /// Return a JSON dump of all public fields. Really, this is a wrapper
    /// around `json::stringify(model.untrusted_data())`.
    ///
    /// Use this function for sending a model to an *untrusted* source, such as
    /// saving to disk or over a network connection.
    fn stringify_untrusted(&self) -> TResult<String> {
        let safe = self.untrusted_data();
        json::stringify(&safe).map_err(|e| toterr!(e))
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
                    $( stringify!($pub_field), )*
                ]
            }

            fn private_fields(&self) -> Vec<&str> {
                vec![
                    $( stringify!($priv_field), )*
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
              type: String,
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
    fn handles_untrusted_data() {
        let mut dog = Dog::blank();
        dog.active = true;
        dog.set("id", &String::from("123")).unwrap();
        dog.set("size", &42).unwrap();
        dog.set("name", &String::from("barky")).unwrap();
        assert_eq!(json::stringify(&dog.untrusted_data()).unwrap(), r#"{"id":"123","size":42}"#);
        assert_eq!(dog.stringify_untrusted().unwrap(), r#"{"id":"123","size":42}"#);
    }

    #[test]
    fn can_serialize_json() {
        let mut dog = Dog::blank();
        dog.set("size", &32).unwrap();
        dog.set("name", &String::from("timmy")).unwrap();
        dog.set("type", &String::from("tiny")).unwrap();
        dog.set("tags", &vec![String::from("canine"), String::from("3-legged")]).unwrap();
        // tests for presence of `extra` fields in JSON (there should be none)
        dog.active = true;
        assert_eq!(dog.stringify_trusted().unwrap(), r#"{"name":"timmy","size":32,"tags":["canine","3-legged"],"type":"tiny"}"#);
        {
            let mut val: &mut json::Value = dog.get_mut("tags").unwrap();
            match val {
                &mut json::Value::Array(ref mut tags) => {
                    tags.push(json::to_val(&String::from("fast")));
                },
                _ => panic!("bad value"),
            };
        }
        assert_eq!(dog.stringify_trusted().unwrap(), r#"{"name":"timmy","size":32,"tags":["canine","3-legged","fast"],"type":"tiny"}"#);
    }
}

