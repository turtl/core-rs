use std::collections::BTreeMap;

use ::error::{TResult, TError};
use ::util::json::{self, Serialize, Deserialize};

/// Defines a key struct, used by many models that have subkey data.
pub struct Key {
    #[allow(dead_code)]
    object_type: String,
    #[allow(dead_code)]
    item_id: String,
    #[allow(dead_code)]
    key: String,
}

//impl Serialize for Key {

//}

/// The Protected trait defines a set of functionality for our models such that
/// they are able to be properly (de)serialized (including encryption/decryption
/// of the model).
///
/// It also defines methods that make it easy to do The Right Thing (c)(r)(tm)
/// when handling protected model data. The goal here is to eliminate all forms
/// of data leaks while providing an interface that's easy to use.
pub trait Protected {
    /// Grab the public fields for this model
    fn public_fields(&self) -> Vec<&str>;

    /// Grab the private fields for this model
    fn private_fields(&self) -> Vec<&str>;

    /// Grab *all* data for this model (safe or not)
    fn data(&self) -> json::Value;

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
    (
        $name:ident,
        ( $( $pub_field:ident: $pub_type:ty ),* ),
        ( $( $priv_field:ident: $priv_type:ty ),* ),
        ( $( $extra_field:ident: $extra_type:ty ),* )
    ) => {
        // Damn, doesn't work:
        //#[derive(::serde::ser::Serialize, ::serde::de::Deserialize)]
        pub struct $name {
            id: Option<String>,
            body: Option<String>,
            $(
                #[allow(dead_code)]
                $pub_field: Option<$pub_type>,
            )*
            $(
                #[allow(dead_code)]
                $priv_field: Option<$priv_type>,
            )*
            $(
                #[allow(dead_code)]
                $extra_field: Option<$extra_type>,
            )*
        }

        impl $name {
            /// Create an instance of this model, with all values set to None
            pub fn new() -> $name {
                let map: ::std::collections::BTreeMap<String, ::util::json::Value> = ::std::collections::BTreeMap::new();
                $name {
                    id: None,
                    body: None,
                    $( $pub_field: None, )*
                    $( $priv_field: None, )*
                    $( $extra_field: None, )*
                }
            }

            /// Grab this model's id
            fn id(&self) -> Option<String> {
                match self.id {
                    Some(ref x) => Some(x.clone()),
                    None => None
                }
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

            fn data(&self) -> ::util::json::Value {
                ::util::json::Value::I64(1)
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

        serializable!($name, ( $( $pub_field ),*, $( $priv_field ),* ) );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::util::json::{self};

    define_protected!(
        Dog,
        ( size: u64 ),
        ( name: String,
          tags: Vec<String> ),
        ()
    );

    #[test]
    fn returns_correct_public_fields() {
        let dog = Dog::new();
        assert_eq!(dog.public_fields(), ["id", "body", "size"]);
    }

    #[test]
    fn returns_correct_private_fields() {
        let dog = Dog::new();
        assert_eq!(dog.private_fields(), ["name", "tags"]);
    }

    #[test]
    fn can_serialize_json() {
        let mut dog = Dog::new();
        dog.size = Some(32);
        dog.name = Some(String::from("timmy"));
        dog.tags = Some(vec![String::from("canine"), String::from("3-legged")]);
        assert_eq!(json::stringify(&dog).unwrap(), r#"{"size":32,"name":"timmy","tags":["canine","3-legged"]}"#);

        dog.tags.as_mut().map(|mut tags| tags.push(String::from("fast")));
        assert_eq!(json::stringify(&dog).unwrap(), r#"{"size":32,"name":"timmy","tags":["canine","3-legged","fast"]}"#);
    }
}

/*
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
    (
        $name:ident,
        ( $( $pub_field:ident: $pub_type:ty ),* ),
        ( $( $priv_field:ident: $priv_type:ty ),* ),
        ( $( $extra_field:ident: $extra_type:ty ),* )
    ) => {
        //#[derive(Serialize, Deserialize)]
        pub struct $name {
            data: ::util::json::Value,
            $( $extra_field: $extra_type, )*
        }

        impl $name {
            pub fn new() -> $name {
                let map: ::std::collections::BTreeMap<String, ::util::json::Value> = ::std::collections::BTreeMap::new();
                $name {
                    data: ::util::json::Value::Object(map),
                    $( $extra_field: $extra_type::new(), )*
                }
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

            fn data(&self) -> &::util::json::Value {
                &self.data
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
*/

