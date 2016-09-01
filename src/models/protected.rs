//! The "Protected" model builds off the Model object to provide a set of tools
//! for handling data safely:
//!
//! - Separation of public and private fields in a model. Essentially, this
//! means fields that will be outside of an encrypted `body` field when
//! serialized (public) and fields that will be *inside* the encrypted `body`
//! field when serialized (private).
//! - (De)serialization. Serializing a protected model means taking its private
//! fields, stringifying them via JSON, and encrypting the resulting string into
//! a `body` field. Deserializing a protected model means reading the `body`
//! field from data, decrypting it, and updating its data with the values from
//! inside the JSON dump. Note that for both operations, the model needs to have
//! a `key` set, which is used as the key for cryptographic operations.
//! - Finding a matching key for an object either from a sibling/parent object
//! or from the current user's keychain.
//!
//! This is mostly provided through the use of a `Protected` trait and a
//! `protected! {} macro, used to wrap around struct definitions to make them
//! protected. This macro also implements the `Debug` trait for the defined
//! models so they don't go around spraying their private fields into debug
//! logs.
//!
//! TODO: key searching (requires user keychain)
//! TODO: ensure key exists
//! TODO: generate_key
//! TODO: detect old format
//! TODO: clone?
//! TODO: generate_subkeys
//! TODO: encrypt_key / decrypt_key

use std::collections::BTreeMap;

use ::std::fmt;

use ::error::{TResult, TError};
use ::models::Model;
use ::util::json::{self};
use ::crypto::{self, CryptoOp};

/// The Protected trait defines a set of functionality for our models such that
/// they are able to be properly (de)serialized (including encryption/decryption
/// of the model).
///
/// It also defines methods that make it easy to do The Right Thing (c)(r)(tm)
/// when handling protected model data. The goal here is to eliminate all forms
/// of data leaks while providing an interface that's easy to use.
pub trait Protected: Model + fmt::Debug {
    /// Get the key for this model
    fn key(&self) -> Option<&Vec<u8>>;

    /// Get this model's "type" (ie, "note", "board", etc).
    fn model_type(&self) -> &str;

    /// Grab the public fields for this model
    fn public_fields(&self) -> Vec<&str>;

    /// Grab the private fields for this model
    fn private_fields(&self) -> Vec<&str>;

    /// Grab the name of this model's table
    fn table(&self) -> String;

    fn get_fields(&self, fields: &Vec<&str>) -> json::Value {
        let mut map: BTreeMap<String, json::Value> = BTreeMap::new();
        let data = self.data();
        for field in fields {
            let val = json::walk(&[field], &data);
            match val {
                Ok(v) => { map.insert(String::from(*field), v.clone()); },
                Err(..) => {}
            }
        }
        json::Value::Object(map)
    }

    /// Grab all public fields for this model as a json Value
    fn untrusted_data(&self) -> json::Value {
        self.get_fields(&self.public_fields())
    }

    /// Grab all private fields for this model as a json Value
    fn trusted_data(&self) -> json::Value {
        self.get_fields(&self.private_fields())
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
        json::stringify(&self.data()).map_err(|e| toterr!(e))
    }

    /// Return a JSON dump of all public fields. Really, this is a wrapper
    /// around `json::stringify(model.untrusted_data())`.
    ///
    /// Use this function for sending a model to an *untrusted* source, such as
    /// saving to disk or over a network connection.
    fn stringify_untrusted(&self) -> TResult<String> {
        json::stringify(&self.untrusted_data()).map_err(|e| toterr!(e))
    }

    /// "Serializes" a model...returns all public data with an *encrypted* set
    /// of private data (in `body`).
    ///
    /// It returns the Value of all *public* fields, but with the `body`
    /// populated with the encrypted data.
    fn serialize(&mut self) -> TResult<json::Value> {
        let body;
        {
            let id = match self.id() {
                Some(x) => x,
                None => String::from("<no id>"),
            };
            let data = self.trusted_data();
            let json = try!(json::stringify(&data));

            let key = match self.key() {
                Some(x) => x,
                None => return Err(TError::BadValue(format!("Protected::serialize() - missing `key` field for {} model {}", self.model_type(), id))),
            };
            body = try!(crypto::encrypt(&key, Vec::from(json.as_bytes()), try!(CryptoOp::new("aes", "gcm"))));
        }
        let body_base64 = try!(crypto::to_base64(&body));
        try!(self.set("body", &body_base64));
        Ok(self.untrusted_data())
    }

    /// "DeSerializes" a model...takes the `body` field, decrypts it, and sets
    /// the values in the decrypted JSON dump back into the model.
    ///
    /// It returns the Value of all public fields.
    fn deserialize(&mut self) -> TResult<json::Value> {
        let id = match self.id() {
            Some(x) => x,
            None => String::from("<no id>"),
        };
        let body = match self.get::<String>("body") {
            Some(x) => try!(crypto::from_base64(&x)),
            None => return Err(TError::MissingField(format!("Protected::deserialize() - missing `body` field for {} model {}", self.model_type(), id))),
        };
        let json_bytes;
        {
            let key = match self.key() {
                Some(x) => x,
                None => return Err(TError::BadValue(format!("Protected::deserialize() - missing `key` field for {} model {}", self.model_type(), id))),
            };
            json_bytes = try!(crypto::decrypt(&key, &body));
        }
        let json_str = try!(String::from_utf8(json_bytes));
        let parsed = try!(json::parse(&json_str));
        try!(self.set_multi(parsed));
        Ok(self.trusted_data())
    }

    fn ensure_key(&mut self) -> Option<&Vec<u8>> {
        let key = self.key();
        key
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
/// fields in your public/private field lists. OR ELSE.
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
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            ( $( $pub_field:ident: $pub_type:ty ),* ),
            ( $( $priv_field:ident: $priv_type:ty ),* ),
            ( $( $extra_field:ident: $extra_type:ty ),* )
        }
    ) => {
        // define the struct
        $(#[$struct_meta])*
        pub struct $name {
            /// Holds our cryptographic key for this model
            key: Option<Vec<u8>>,

            /// Defines what type of model we have (note, board, etc)
            model_type: String,

            /// Holds our model's actual data
            _data: ::util::json::Value,

            /// Our event emitter!
            emitter: ::util::event::EventEmitter,

            $( $extra_field: $extra_type, )*
        }

        // run our implementations
        protected!([IMPL ( $name ), ( $( $pub_field ),* ), ( $( $priv_field ),* ), ( $( $extra_field ),* )]);
    };

    // implementation
    (
        [IMPL ( $name:ident ),
              ( $( $pub_field:ident ),* ),
              ( $( $priv_field:ident ),* ),
              ( $( $extra_field:ident ),* )]

    ) => {
        // import our Model trait without polluting our namespace
        use ::models::Model as PModel;
        use ::util::event::Emitter as PEmitter;

        impl $name {
            /// Create an instance of this model, with all values set to None
            #[allow(dead_code)]
            pub fn blank() -> $name {
                $name {
                    key: None,
                    model_type: String::from(stringify!($name)),
                    _data: ::util::json::obj(),
                    emitter: ::util::event::EventEmitter::new(),
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

        impl PEmitter for $name {
            fn bindings(&mut self) -> &mut ::std::collections::HashMap<String, Vec<::util::event::Callback>> {
                self.emitter.bindings()
            }
        }

        impl PModel for $name {
            fn data(&self) -> &::util::json::Value {
                &self._data
            }

            fn data_mut(&mut self) -> &mut ::util::json::Value {
                &mut self._data
            }
        }

        // make sure printing out a model doesn't leak data
        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                let id = match self.id() {
                    Some(x) => x,
                    None => String::from("<no id>"),
                };
                write!(f, "{}: ({})", self.model_type(), id)
            }
        }

        impl Protected for $name {
            fn key(&self) -> Option<&Vec<u8>> {
                match self.key {
                    Some(ref x) => Some(x),
                    None => None,
                }
            }

            fn model_type(&self) -> &str {
                &self.model_type[..]
            }

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

            fn table(&self) -> String {
                String::from(stringify!($name)).to_lowercase()
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
    use ::crypto;

    protected!{
        pub struct Dog {
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

    #[test]
    fn encrypts_decrypts() {
        let json = String::from(r#"{"size":69,"name":"barky","type":"canadian","tags":["flappy","noisy"]}"#);
        let value = json::parse(&json).unwrap();
        let mut dog = Dog::new(value).unwrap();
        let key = crypto::random_key().unwrap();
        dog.key = Some(key.clone());
        let serialized = dog.serialize().unwrap();

        let body: String = json::get(&["body"], &serialized).unwrap();
        match json::get::<String>(&["name"], &serialized) {
            Ok(..) => panic!("data from Protected::serialize() contains private fields"),
            Err(e) => match e {
                json::JSONError::NotFound(..) => (),
                _ => panic!("error while testing data returned from Protected::serialize() - {}", e),
            }
        }
        assert_eq!(body, dog.get::<String>("body").unwrap());

        let mut dog2 = Dog::new(dog.untrusted_data().clone()).unwrap();
        dog2.set_multi(dog.untrusted_data()).unwrap();
        assert_eq!(dog.stringify_untrusted().unwrap(), dog2.stringify_untrusted().unwrap());
        dog2.key = Some(key.clone());
        assert_eq!(dog2.get::<i64>("size").unwrap(), 69);
        assert_eq!(dog2.get::<String>("name"), None);
        assert_eq!(dog2.get::<String>("type"), None);
        assert_eq!(dog2.get::<Vec<String>>("tags"), None);
        let res = dog2.deserialize().unwrap();
        assert_eq!(dog.stringify_trusted().unwrap(), dog2.stringify_trusted().unwrap());
        assert_eq!(json::get::<String>(&["name"], &res).unwrap(), "barky");
        assert_eq!(json::get::<String>(&["type"], &res).unwrap(), "canadian");
        assert_eq!(dog2.get::<i64>("size").unwrap(), 69);
        assert_eq!(dog2.get::<String>("name").unwrap(), "barky");
        assert_eq!(dog2.get::<String>("type").unwrap(), "canadian");
        assert_eq!(dog2.get::<Vec<String>>("tags").unwrap(), vec!["flappy", "noisy"]);
    }
}

