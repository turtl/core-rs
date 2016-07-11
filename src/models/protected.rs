use std::collections::BTreeMap;

use ::error::{TResult, TError};
use ::models::Model;
use ::util::json::{self};

/// Defines a key struct, used by many models that have subkey data.
serializable! {
    pub struct Key {
        type_: String,
        item_id: String,
        key: String,
    }
}

/// The Protected trait defines a set of functionality for our models such that
/// they are able to be properly (de)serialized (including encryption/decryption
/// of the model).
///
/// It also defines methods that make it easy to do The Right Thing (c)(r)(tm)
/// when handling protected model data. The goal here is to eliminate all forms
/// of data leaks while providing an interface that's easy to use.
pub trait Protected: Model {
    /// Grab this model's id
    fn id(&self) -> Option<String>;

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
/// define_protected!(Squirrel, (size: u64), (name: String), ());
/// # }
/// ```
#[macro_export]
macro_rules! define_protected {
    // pub
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ( $( $pub_field:ident: $pub_type:ty ),* ),
            ( $( $priv_field:ident: $priv_type:ty ),* ),
            ( $( $extra_field:ident: $extra_type:ty ),* )
        }
    ) => {
        serializable! {
            #[derive(Default)]
            pub struct $name {
                ($( $extra_field: $extra_type ),*)
                id: Option<String>,
                body: Option<String>,
                $( $pub_field: Option<$pub_type>, )*
                $( $priv_field: Option<$priv_type>, )*
            }
        }

        define_protected!([IMPL ( $name ), ( $( $pub_field ),* ), ( $( $priv_field ),* )]);
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
        serializable! {
            #[derive(Default)]
            struct $name {
                ($( $extra_field: $extra_type ),*)
                id: Option<String>,
                body: Option<String>,
                $( $pub_field: Option<$pub_type>, )*
                $( $priv_field: Option<$priv_type>, )*
            }
        }

        define_protected!([IMPL ( $name ), ( $( $pub_field ),* ), ( $( $priv_field ),* )]);
    };

    // implementation
    (
        [IMPL ( $name:ident ),
              ( $( $pub_field:ident ),* ),
              ( $( $priv_field:ident ),* )]
    ) => {
        impl $name {
            /// Create an instance of this model, with all values set to None
            pub fn new() -> $name {
                Default::default()
            }

        }

        impl ::models::Model for $name {
            fn data(&self) -> ::util::json::Value {
                ::util::json::to_val(self)
            }
        }

        impl Protected for $name {
            fn id(&self) -> Option<String> {
                match self.id {
                    Some(ref x) => Some(x.clone()),
                    None => None
                }
            }

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

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                let id = match self.id() {
                    Some(x) => x,
                    None => String::from("<no id>"),
                };
                write!(f, "{}: ({})", stringify!($name), id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json::{self};

    define_protected!{
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
        let dog = Dog::new();
        assert_eq!(dog.public_fields(), ["id", "body", "size"]);
    }

    #[test]
    fn returns_correct_private_fields() {
        let dog = Dog::new();
        assert_eq!(dog.private_fields(), ["name", "type", "tags"]);
    }

    #[test]
    fn returns_data() {
        let mut dog = Dog::new();
        dog.name = Some(String::from("barky"));
        let data = dog.data();
        let name: String = json::get(&["name"], &data).unwrap();
        let active: json::JResult<bool> = json::get(&["active"], &data);
        assert_eq!(name, "barky");
        match active {
            Ok(..) => panic!("Found `active` which is an extra field and should not be serialized"),
            Err(e) => match e {
                json::JSONError::NotFound(..) => {},
                _ => panic!("Got an error whiel looking for `active` field (should have gotten JSONError::NotFound)"),
            }
        }
    }

    #[test]
    fn handles_safe_data() {
        let mut dog = Dog::new();
        dog.id = Some(String::from("123"));
        dog.size = Some(42);
        dog.name = Some(String::from("barky"));
        assert_eq!(dog.safe_stringify().unwrap(), r#"{"body":null,"id":"123","size":42}"#);
    }

    #[test]
    fn returns_an_id() {
        let mut dog = Dog::new();
        dog.id = Some(String::from("123"));
        let id = dog.id().unwrap();
        assert_eq!(id, "123");
    }

    #[test]
    fn can_serialize_json() {
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
    }
}

