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

use ::std::collections::{HashMap};
use ::std::fmt;

use ::futures::{future, Future};

use ::jedi::{self, Value, Map as JsonMap};

use ::error::{TResult, TError, TFutureResult};
use ::turtl::Turtl;
use ::models::model::Model;
use ::crypto::{self, Key, CryptoOp};
use ::models::keychain::{KeyRef, Keychain};

// -----------------------------------------------------------------------------
// NOTE: [encrypt|decrypt]_key() do not use async crypto.
//
// the rationale behind this is that the data they operate on is predictably
// small, and therefor has predictable performance.
//
// consider these functions conscientious objectors to worker crypto.
// -----------------------------------------------------------------------------
/// Decrypt an encrypted key, generally as part of a Protected.keys collection
pub fn decrypt_key(decrypting_key: &Key, encrypted_key: &String) -> TResult<Key> {
    let raw = crypto::from_base64(encrypted_key)?;
    let decrypted = crypto::decrypt(decrypting_key, raw)?;
    Ok(Key::new(decrypted))
}

/// Encrypt a decrypted key, mainly for storage self-decrypting keys with models
pub fn encrypt_key(encrypting_key: &Key, key_to_encrypt: Key) -> TResult<String> {
    let encrypted = crypto::encrypt(encrypting_key, key_to_encrypt.into_data(), crypto::CryptoOp::new("chacha20poly1305")?)?;
    let converted = crypto::to_base64(&encrypted)?;
    Ok(converted)
}
// -----------------------------------------------------------------------------

/// Map over a vec of Protected models, deserialize()ing them in worker threads
/// and returning the resulting deserialized models as a vec in a future result
pub fn map_deserialize<T>(turtl: &Turtl, vec: Vec<T>) -> TResult<Vec<T>>
    where T: Protected + Send + Sync + 'static
{
    // Allows our future to collect a single result type which can then be
    // filtered at the end so we only return models that successfully
    // deserialized
    enum DeserializeResult<T> {
        Model(T),
        Failed,
    }

    debug!("protected::map_deserialize() -- starting on {} items", vec.len());
    let ref work = turtl.work;
    let futures = vec.into_iter()
        .map(|mut model| {
            // don't bother with models that don't have a key...
            if model.key().is_none() {
                warn!("map_deserialize: model {:?} has no key", model.id());
                return FOk!(DeserializeResult::Failed);
            }
            let mut model_clone = ftry!(model.clone());
            let model_type = String::from(model.model_type());
            let model_id = model.id().unwrap().clone();
            // run the deserialize, return the result into our future chain
            work.run_async(move || model_clone.deserialize())
                .and_then(move |item_mapped: Value| -> TFutureResult<DeserializeResult<T>> {
                    ftry!(model.merge_fields(&item_mapped));
                    FOk!(DeserializeResult::Model(model))
                })
                .or_else(move |e| -> TFutureResult<DeserializeResult<T>> {
                    error!("protected::map_deserialize() -- error deserializing {} model ({:?}): {}", model_type, model_id, e);
                    FOk!(DeserializeResult::Failed)
                })
                .boxed()
        })
        .collect::<Vec<_>>();
    // wait for all our futures to finish. this will return them in order of
    // starting (NOT order of completion).
    let mapped = future::join_all(futures).wait()?;
    // only return the models that succeeded deserialization, preserving
    // the order.
    // TODO: benchmark if using an iterator is faster here
    let mut final_models = Vec::with_capacity(mapped.len());
    for result in mapped {
        match result {
            DeserializeResult::Model(m) => { final_models.push(m) },
            DeserializeResult::Failed => {},
        }
    }
    debug!("protected::map_deserialize() -- finishing");
    Ok(final_models)
}


/// Allows a model to expose a key search
pub trait Keyfinder {
    /// Grabs a model's key search. This is mainly used for things like Note,
    /// which will often search in spaces/boards for a key.
    fn get_key_search(&self, _: &Turtl) -> TResult<Keychain> {
        Ok(Keychain::new())
    }

    /// When a model is about to be saved, it will often want to encrypt its
    /// key with the keys of other objects and store it in itself (the concept
    /// of subkeys). Here, we allow an override for this.
    fn get_keyrefs(&self, _: &Turtl) -> TResult<Vec<KeyRef<Key>>> {
        Ok(Vec::new())
    }

    /// Whether or not this model's key should be added directly to the user's
    /// Keychain. Default to NO
    fn add_to_keychain(&self) -> bool {
        false
    }
}

/// The Protected trait defines a set of functionality for our models such that
/// they are able to be properly (de)serialized (including encryption/decryption
/// of the model).
///
/// It also defines methods that make it easy to do The Right Thing (c)(r)(tm)
/// when handling protected model data. The goal here is to eliminate all forms
/// of data leaks while providing an interface that's easy to use.
pub trait Protected: Model + fmt::Debug {
    /// Get the key for this model
    fn key(&self) -> Option<&Key>;

    /// Get the key for this model, or return an error of it's missing
    fn key_or_else(&self) -> TResult<Key>;

    /// Set this model's key
    fn set_key(&mut self, key: Option<Key>);

    /// Get this model's "type" (ie, "note", "board", etc).
    fn model_type(&self) -> String;

    /// Grab the public fields for this model
    fn public_fields(&self) -> Vec<&'static str>;

    /// Grab the private fields for this model
    fn private_fields(&self) -> Vec<&'static str>;

    /// Grab the fields names of any child models this model has
    fn submodel_fields(&self) -> Vec<&'static str>;

    /// Get (JSON) data from one of our submodels
    fn submodel_data(&self, field: &str, private: bool) -> TResult<Value>;

    /// Sets our key into all our submodels
    fn _set_key_on_submodels(&mut self);

    /// Serializes our submodels
    fn serialize_submodels(&mut self) -> TResult<()>;

    /// Deserializes our submodels
    fn deserialize_submodels(&mut self) -> TResult<()>;

    /// Clone this protected model
    fn clone(&self) -> TResult<Self>;

    /// Either grab the existing or generate a new key for this model
    fn generate_key(&mut self) -> TResult<&Key>;

    /// Get the model's body data
    fn get_keys<'a>(&'a self) -> Option<&'a Vec<HashMap<String, String>>>;

    /// Set the keys for this model
    fn set_keys(&mut self, keydata: Vec<HashMap<String, String>>);

    /// Get the model's body data
    fn get_body<'a>(&'a self) -> Option<&'a String>;

    /// Set the model's body data
    fn set_body(&mut self, body: String);

    /// Clear out the model's body data
    fn clear_body(&mut self);

    /// Merge a Value object into this model.
    fn merge_fields(&mut self, data: &Value) -> TResult<()>;

    /// Get a set of fields and return them as a JSON Value
    fn get_fields(&self, fields: &Vec<&str>) -> TResult<JsonMap<String, Value>> {
        let mut map: JsonMap<String, jedi::Value> = JsonMap::new();
        let data = jedi::to_val(self)?;
        for field in fields {
            let val = jedi::walk(&[field], &data);
            match val {
                Ok(v) => { map.insert(String::from(*field), v.clone()); },
                Err(..) => {}
            }
        }
        Ok(map)
    }

    /// Get a set of fields and return them as a JSON Value
    fn get_serializable_data(&self, private: bool) -> TResult<Value> {
        let fields = if private {
            self.private_fields()
        } else {
            self.public_fields()
        };
        let mut map = self.get_fields(&fields)?;
        let submodels = self.submodel_fields();
        // shove in our submodels' public/private data
        for field in submodels {
            let val: TResult<Value> = self.submodel_data(field, private);
            match val {
                Ok(v) => {
                    if !v.is_null() {
                        map.insert(String::from(field), v);
                    }
                },
                Err(..) => {},
            }
        }
        Ok(Value::Object(map))
    }

    /// Grab all public fields for this model as a json Value
    ///
    /// NOTE: Don't use this directly. Use `data_for_storage()` instead!
    fn _public_data(&self) -> TResult<Value> {
        self.get_serializable_data(false)
    }

    /// Grab all private fields for this model as a json Value
    ///
    /// NOTE: Don't use this directly. Use `data()` instead!
    fn _private_data(&self) -> TResult<Value> {
        self.get_serializable_data(true)
    }

    /// Grab a JSON Value representation of ALL this model's data
    fn data(&self) -> TResult<Value> {
        Ok(jedi::to_val(self)?)
    }

    /// Grab all public fields for this model as a JSON Value.
    fn data_for_storage(&self) -> TResult<Value> {
        self._public_data()
    }

    /// Return a JSON dump of all fields. Really, this is a wrapper around
    /// `jedi::stringify(model.data())`.
    ///
    /// Use this function when sending a model to a trusted source (ie inproc
    /// messaging to our view layer).
    ///
    /// __NEVER__ use this function to save data to disk or transmit over a
    /// network connection.
    fn stringify_unsafe(&self) -> TResult<String> {
        jedi::stringify(&self.data()?).map_err(|e| toterr!(e))
    }

    /// Return a JSON dump of all public fields. Really, this is a wrapper
    /// around `jedi::stringify(model.data_for_storage())`.
    ///
    /// Use this function for sending a model to an *untrusted* source, such as
    /// saving to disk or over a network connection.
    fn stringify_for_storage(&self) -> TResult<String> {
        jedi::stringify(&self.data_for_storage()?).map_err(|e| toterr!(e))
    }

    /// "Serializes" a model...returns all public data with an *encrypted* set
    /// of private data (in `body`).
    ///
    /// It returns the Value of all *public* fields, but with the `body`
    /// populated with the encrypted data.
    fn serialize(&mut self) -> TResult<Value> {
        if self.key().is_none() {
            return TErr!(TError::MissingField(format!("model {:?} missing `key`", self.id())));
        }
        self.serialize_submodels()?;
        let body;
        {
            let fakeid = String::from("<no id>");
            let id = match self.id() {
                Some(x) => x,
                None => &fakeid,
            };
            let data = self._private_data()?;
            let json = jedi::stringify(&data)?;

            let key: &Key = match self.key() {
                Some(x) => x,
                None => return TErr!(TError::MissingField(format!("model {} ({}) missing `key`", id, self.model_type()))),
            };
            // government surveillance agencies *HATE* him!!!!1
            body = crypto::encrypt(&key, Vec::from(json.as_bytes()), CryptoOp::new("chacha20poly1305")?)?;
        }
        let body_base64 = crypto::to_base64(&body)?;
        self.set_body(body_base64);
        Ok(self.data_for_storage()?)
    }

    /// "DeSerializes" a model...takes the `body` field, decrypts it, and
    /// returns a JSON Value of the public/private fields.
    fn deserialize(&mut self) -> TResult<Value> {
        if self.key().is_none() {
            return TErr!(TError::MissingField(format!("model {:?} ({}) missing `key`", self.id(), self.model_type())));
        }
        self.deserialize_submodels()?;
        let fakeid = String::from("<no id>");
        let json_bytes = {
            let id = match self.id() {
                Some(x) => x,
                None => &fakeid,
            };
            let body: Vec<u8> = match self.get_body() {
                Some(x) => crypto::from_base64(x)?,
                None => return TErr!(TError::MissingField(format!("model {} ({}) missing `body`", id, self.model_type()))),
            };
            let key: &Key = match self.key() {
                Some(x) => x,
                None => return TErr!(TError::MissingField(format!("model {} ({}) missing `key`", id, self.model_type()))),
            };
            crypto::decrypt(key, body)?
        };
        let json_str: String = match String::from_utf8(json_bytes) {
            Ok(x) => x,
            Err(e) => return TErr!(TError::BadValue(format!("error decoding UTF8 string: {}", e))),
        };
        let parsed: Value = match jedi::parse(&json_str) {
            Ok(x) => x,
            Err(e) => {
                error!("protected.deserialize() -- error parsing JSON for {} model {:?}: {}", self.model_type(), self.id(), e);
                let err: TError = From::from(e);
                return TErr!(err);
            },
        };
        self.merge_fields(&parsed)?;
        Ok(self._private_data()?)
    }

    /// Given a set of keydata, replace the self.keys object
    fn generate_subkeys(&mut self, keydata: &Vec<KeyRef<Key>>) -> TResult<()> {
        if self.key().is_none() {
            return TErr!(TError::MissingData(format!("Protected.generate_subkeys() -- missing `key` (type: {}, id {:?})", self.model_type(), self.id())));
        }
        let model_key = self.key().unwrap().clone();
        let mut encrypted: Vec<HashMap<String, String>> = Vec::with_capacity(keydata.len());
        for key in keydata {
            let enc = encrypt_key(&key.k, model_key.clone())?;
            let mut hash: HashMap<String, String> = HashMap::with_capacity(2);
            hash.insert(key.ty.clone(), key.id.clone());
            hash.insert(String::from("k"), enc);
            encrypted.push(hash);
        }
        self.set_keys(encrypted);
        Ok(())
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
#[macro_export]
macro_rules! protected {
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            $( $inner:tt )*
        }
    ) => {
        model! {
            $(#[$struct_meta])*
            #[derive(Protected)]
            pub struct $name {
                #[serde(skip)]
                _key: Option<::crypto::Key>,

                #[serde(skip_serializing_if = "Option::is_none")]
                #[protected_field(public)]
                keys: Option<Vec<::std::collections::HashMap<String, String>>>,
                #[protected_field(public)]
                body: Option<String>, 

                $( $inner )*
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi;
    use ::crypto::{self, Key};
    use ::models::model::Model;
    use ::models::keychain::KeyRef;
    use ::models::note::Note;

    protected! {
        #[derive(Serialize, Deserialize)]
        pub struct Dog {
            #[serde(skip)]
            pub active: bool,

            #[protected_field(public)]
            pub size: Option<i64>,

            #[protected_field(private)]
            pub name: Option<String>,
            #[serde(rename = "type")]
            #[protected_field(private)]
            pub type_: Option<String>,
            #[protected_field(private)]
            pub tags: Option<Vec<String>>,
        }
    }

    protected! {
        #[derive(Serialize, Deserialize)]
        pub struct Junkyard {
            #[protected_field(public)]
            pub name: Option<String>,

            // Uhhh, I'm sorry. Is this not a junkyard?!
            #[protected_field(private, submodel)]
            pub dog: Option<Dog>,
        }
    }

    #[test]
    fn returns_correct_public_fields() {
        let dog = Dog::new();
        assert_eq!(dog.public_fields(), ["id", "keys", "body", "size"]);
    }

    #[test]
    fn returns_correct_private_fields() {
        let dog = Dog::new();
        assert_eq!(dog.private_fields(), ["name", "type", "tags"]);
    }

    #[test]
    fn handles_public_data() {
        let mut dog = Dog::new();
        dog.active = true;
        dog.id = Some(String::from("123"));
        dog.size = Some(42i64);
        dog.name = Some(String::from("barky"));
        assert_eq!(jedi::stringify(&dog.data_for_storage().unwrap()).unwrap(), r#"{"body":null,"id":"123","size":42}"#);
        assert_eq!(dog.stringify_for_storage().unwrap(), r#"{"body":null,"id":"123","size":42}"#);
    }

    #[test]
    fn can_serialize_json() {
        let mut dog = Dog::new();
        dog.size = Some(32i64);
        dog.name = Some(String::from("timmy"));
        dog.type_ = Some(String::from("tiny"));
        dog.tags = Some(vec![String::from("canine"), String::from("3-legged")]);
        // tests for presence of `extra` fields in JSON (there should be none)
        dog.active = true;
        assert_eq!(dog.stringify_unsafe().unwrap(), r#"{"body":null,"name":"timmy","size":32,"tags":["canine","3-legged"],"type":"tiny"}"#);
        {
            let mut tags: &mut Vec<String> = dog.tags.as_mut().unwrap();
            tags.push(String::from("fast"));
        }
        assert_eq!(dog.stringify_unsafe().unwrap(), r#"{"body":null,"name":"timmy","size":32,"tags":["canine","3-legged","fast"],"type":"tiny"}"#);
    }

    #[test]
    fn deserializes_keys() {
        let json = String::from(r#"{"id":"015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","user_id":51,"file":{},"keys":[{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAzSgseWF4MMXhZ8RDg3igwoghg9vAdlwaG70EwncM9odiZ6rQq5U/Dv1ZXTUgOGolwEGZ7PjFYw8IJhQ10="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAysMP4OBviiXtL86+pmH2jpIYH9D5AsbpLTQ7GoTXsugfvyM3hhJuUNsBQlPVtbOqATaS87Mx3sDnQXEFQ="}],"mod":1498539524,"body":"AAYBAAzH4KVxGsdEq2PhfjX6dTSmPiVye8gv+Yp457UiYEce5jrL6T1K4WyNnvZizqeKOPGyMnqAtBxxNrClfwV4YVdlNDAQQAKQSSln+K0CvSgcIdC8mRCHqOobFWazYy7pS1SlKrNz9tBnJXjvJOzjRjI4GAGVVNj9t2YoJfFFDVFi1slTEC8SRDXj82AvaYIoGjF1bnw0FY4d3AOiigdJa4s5VRbsGG/75djUinn0i1avSqfdm5E="}"#);
        let note: Note = jedi::parse(&json).unwrap();
        let keys = note.get_keys();
        assert_eq!(keys.unwrap().len(), 2);
    }

    #[test]
    fn clones() {
        let mut note: Note = jedi::parse(&String::from(r#"{"id":"015caf78be502af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a018e","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":null,"user_id":51,"file":{},"keys":[{"k":"AAYBAAzuWB81LF46TLQ0b9aibwlL4lT5FTxw1UNxtUNKA2zuzW91drujc53uMQipFhcq6s6Ff9mDQr0Ew5H7Guw=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"mod":1497592545,"body":"AAYBAAzChZjAOGoAQ0MMjLofXXHarNfUu9Eqlv/063dUH4kbrp8Mnmw+XIn7LxAHloxdMpdiVDz5SAcLyy5DftjOjEEwKfylexz+C9zq5CQSjsQzuRQYMxD7TAwiJZLd+CsM1msek0kkhIB2whG6plMC8Hlyu1bMdcvWJ3B7Oonp89V57ycedVsSMWE28ablc3X3aKO8LRjCnoZlOK/UbZZYQnkm4roGV8dWlbKziTHm8R9ctBrxceo5ky3molooQ6GPKIPbm+lomsyrGDBG4DBDd7KlMJ1LCcsXzYWLnqvQyYny2ly37l5x3Y4dOcZVZ0gxkSzvHe37AzQl"}"#)).unwrap();
        note.generate_key().unwrap();
        let note2 = note.clone().unwrap();
        assert_eq!(note2.space_id, String::from("015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"));
        assert_eq!(note2.board_id, None);
        assert_eq!(note.key(), note2.key());
    }

    #[test]
    fn encrypts_decrypts() {
        let json = String::from(r#"{"size":69,"name":"barky","type":"canadian","tags":["flappy","noisy"]}"#);
        let mut dog: Dog = jedi::parse(&json).unwrap();
        let key = crypto::Key::random().unwrap();
        dog.set_key(Some(key.clone()));
        let serialized = dog.serialize().unwrap();

        let body: String = jedi::get(&["body"], &serialized).unwrap();
        match jedi::get::<String>(&["name"], &serialized) {
            Ok(..) => panic!("data from Protected::serialize() contains private fields"),
            Err(e) => match e {
                jedi::JSONError::NotFound(..) => (),
                _ => panic!("error while testing data returned from Protected::serialize() - {}", e),
            }
        }
        assert_eq!(&body, dog.body.as_ref().unwrap());

        let mut dog2 = Dog::clone_from(dog.data_for_storage().unwrap()).unwrap();
        assert_eq!(dog.stringify_for_storage().unwrap(), dog2.stringify_for_storage().unwrap());
        dog2.set_key(Some(key.clone()));
        assert_eq!(dog2.size.unwrap(), 69);
        assert_eq!(dog2.name, None);
        assert_eq!(dog2.type_, None);
        assert_eq!(dog2.tags, None);
        let res = dog2.deserialize().unwrap();
        assert_eq!(dog.stringify_unsafe().unwrap(), dog2.stringify_unsafe().unwrap());
        assert_eq!(jedi::get::<String>(&["name"], &res).unwrap(), "barky");
        assert_eq!(jedi::get::<String>(&["type"], &res).unwrap(), "canadian");
        assert_eq!(dog2.size.unwrap(), 69);
        assert_eq!(dog2.name.unwrap(), String::from("barky"));
        assert_eq!(dog2.type_.unwrap(), String::from("canadian"));
        assert_eq!(dog2.tags.unwrap(), vec!["flappy", "noisy"]);
    }

    #[test]
    fn decrypts_utf8() {
        let mut note: Note = jedi::parse(&String::from(r#"{"id":"015ce7ea7f742af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a00aa","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","user_id":51,"file":{},"keys":[{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAyAjxgMehPHn+xMYOAW8/aGgxRrQN8FvB/lQoI2uX7khX8eQi2un4eFa73kboM6UAiCvSKGnmX9DNIwGk4="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAy/IWmcQN42Iva4LqNg0eDAIU4slpoAZ/8487NJxXjISkd4HmOLxBPg/Lbf7pa5E/MB7pOsTHGLENcDoWw="},{"s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","k":"AAYBAAyAjxgMehPHn+xMYOAW8/aGgxRrQN8FvB/lQoI2uX7khX8eQi2un4eFa73kboM6UAiCvSKGnmX9DNIwGk4="},{"b":"015bac2244ea4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa30034","k":"AAYBAAy/IWmcQN42Iva4LqNg0eDAIU4slpoAZ/8487NJxXjISkd4HmOLxBPg/Lbf7pa5E/MB7pOsTHGLENcDoWw="}],"mod":1498539524,"body":"AAYBAAw/xkOg209rBB+kSM2o8aKTvzsDuY0bcwN7W5zuwf+kFPCAEH/ERnxIbO1SOE4+Z3+WUwRDhsOSx9VR2gTON9bcMCWUiS1DP5oNWhLZ9HZxvF1dlpN6jnfTokeE7Aw0uVjIrSma3AW7vaA3tTokZdW9j7fpqzBYGZXrZT6+1/RAsKrHiayVGZdR//4iKoRZeysgsu8Hn6aaMhgJ+tSV9Kz7MZeKHJb2fxWVr1BTZQeRWoXKhjU="}"#)).unwrap();
        let key = Key::new(crypto::from_base64(&String::from("VAkQBuwoPXAQdDOIHZ/ItNWL0xZh+qBT5GKtj92HZ/8=")).unwrap());
        note.set_key(Some(key));
        note.deserialize().unwrap();
        assert_eq!(note.title.unwrap(), "\u{2620} my favorite site \u{2620}");
    }

    #[test]
    fn decrypts_clones() {
        let mut note: Note = jedi::parse(&String::from(r#"{"id":"015caf7c5f4d2af6297cf0cc29180f9cc45f4c80e5b30238581f845367f9c404ef3fb8fb0a5a022b","space_id":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e","board_id":null,"user_id":51,"file":{},"keys":[{"k":"AAYBAAw652QkoEkgpi6TYrW3TTmDGz6fJk2rIB+wP2aguuxXFZOd7AL9p/Ka2tQ0lpEYzoAQEV7e03CgIoTaA0I=","s":"015bac22440a4944baee41b88207731eaeb7e2cc5c955fb8a05b028c1409aaf55024f5d26fa3001e"}],"mod":1497592783,"body":"AAYBAAzNUsLEsUAek2V9bm6mRl4gihS9R54JtmbdTriCIXMBjqBrs6BPwa+BYYh//z+ISzYqU2RsdZNMnPXDyzLnvNYu97wYIprQA4KhbZrQpYvlJSeYb/ZZOlv9fEwuz8nCvovl7gPFqpQNRNFfj+QRgTmuT6nq2E9C/5R/HFeKV7paD6/xquD1BkLGpsKDGzqG07q7UMABWjFEXeMszBanCLLVl/CPy7l04hV5f5kx"}"#)).unwrap();
        let key = Key::new(crypto::from_base64(&String::from("smZYz6J4INwhVxJF5XcUyeojdOlKV0o7jKm4C2tJ7V0=")).unwrap());
        note.set_key(Some(key));
        let mut note_clone = note.clone().unwrap();
        note_clone.deserialize().unwrap();
        assert_eq!(note_clone.type_.unwrap(), "text");
        assert_eq!(note_clone.text.unwrap(), "PEOPLE TAKE U MORE SRSLY");
    }

    #[test]
    fn recursive_serialization() {
        let mut junkyard: Junkyard = jedi::parse(&String::from(r#"{"name":"US political system","dog":{"size":69,"name":"Gerard","type":"chowchow","tags":["bites","stubborn","furry"]}}"#)).unwrap();
        assert_eq!(junkyard.stringify_for_storage().unwrap(), String::from(r#"{"body":null,"dog":{"body":null,"size":69},"name":"US political system"}"#));
        assert_eq!(junkyard.stringify_unsafe().unwrap(), String::from(r#"{"body":null,"dog":{"body":null,"name":"Gerard","size":69,"tags":["bites","stubborn","furry"],"type":"chowchow"},"name":"US political system"}"#));
        junkyard.generate_key().unwrap();
        junkyard.serialize().unwrap();

        // ok, we serialized some stuff, let's see if we did it recursively AND
        // if we can undo it
        let storage = junkyard.stringify_for_storage().unwrap();

        let mut junkyard2: Junkyard = jedi::parse(&storage).unwrap();
        assert_eq!(junkyard2.dog.as_ref().unwrap().size.as_ref().unwrap(), &69);
        junkyard2.set_key(Some(junkyard.key().unwrap().clone()));
        junkyard2.deserialize().unwrap();
        let mut dog = junkyard2.dog.as_mut().unwrap();
        assert_eq!(dog.size.as_ref().unwrap(), &69);
        assert_eq!(dog.name.as_ref().unwrap(), &String::from("Gerard"));
        assert_eq!(dog.type_.as_ref().unwrap(), &String::from("chowchow"));
        assert_eq!(dog.size.as_ref().unwrap(), &69);
        dog.body = None;
        assert_eq!(dog.stringify_unsafe().unwrap(), String::from(r#"{"body":null,"name":"Gerard","size":69,"tags":["bites","stubborn","furry"],"type":"chowchow"}"#));
    }

    #[test]
    fn generate_subkeys() {
        let mut dog: Dog = jedi::parse(&String::from(r#"{"size":30,"name":"dog","type":"shiba"}"#)).unwrap();
        dog.generate_key().unwrap();
        let mut subkeys: Vec<KeyRef<Key>> = Vec::new();
        let key1 = Key::new(crypto::from_base64(&String::from("n1OBWSG3LqwqoL/Oo8nyUPJp8fl/8Wig6kWpS45YW1U=")).unwrap());
        let key2 = Key::new(crypto::from_base64(&String::from("mbYnVxRr4wJ+Zh0tK96rM9dqveW5efJligps4IHoVW4=")).unwrap());
        subkeys.push(KeyRef::new(String::from("6969"), String::from("b"), key1));
        subkeys.push(KeyRef::new(String::from("1234"), String::from("b"), key2));
        dog.generate_subkeys(&subkeys).unwrap();
        // not the best test, but whatever. i suppose i could write a base64
        // regex. feeling lazy tonight.
        assert_eq!(dog.keys.as_ref().unwrap().len(), 2);
    }
}

