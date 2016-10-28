//! The Model type defines an object that maps user's data (a note, a board,
//! etc etc) to a database table and/or a set of methods/operations that can be
//! run on that data.
//!
//! The most important aspect of models is that they are (De)Serialize(able),
//! making them easy to save/load to various data sources.

use ::std::sync::RwLock;

use ::time;
use ::serde::ser::Serialize;
use ::serde::de::Deserialize;
use ::jedi;

use ::error::{TError, TResult};
use ::util::event::Emitter;

lazy_static! {
    /// create a static/global cid counter
    static ref CID_COUNTER: RwLock<u32> = RwLock::new(0);

    /// holds our app's client id
    static ref CLIENT_ID: RwLock<Option<String>> = RwLock::new(None);
}

/// Set the model system's client id
pub fn get_client_id() -> Option<String> {
    let guard = (*CLIENT_ID).read().unwrap();
    (*guard).clone()
}

/// Set the model system's client id
pub fn set_client_id(id: String) -> TResult<()> {
    debug!("model -- set_client_id(): {}", id);
    let mut guard = (*CLIENT_ID).write().unwrap();
    *guard = Some(id);
    Ok(())
}

/// Create a turtl object id from a client id
pub fn cid() -> TResult<String> {
    let client_id = match get_client_id() {
        Some(ref x) => x.clone(),
        None => return Err(TError::MissingData(format!("model: CLIENT_ID missing"))),
    };
    let mut counter_guard = (*CID_COUNTER).write().unwrap();
    let counter: u32 = counter_guard.clone();
    (*counter_guard) += 1;
    let now = time::get_time();
    let milis = ((now.sec as u64) * 1000) + ((now.nsec as u64) / 1000000);
    let mut cid = format!("{:01$x}", milis, 12);
    let counter_str = format!("{:01$x}", (counter & 65535), 4);
    cid.push_str(&client_id[..]);
    cid.push_str(&counter_str[..]);
    Ok(cid)
}

/// The model trait defines an interface for (de)serializable objects that track
/// their changes via eventing.
pub trait Model: Emitter + Serialize + Deserialize {
    /// Get the fields in this model
    fn fields(&self) -> Vec<&'static str>;

    /// Get this model's ID
    fn id<'a>(&'a self) -> Option<&'a String>;

    /// Merge another model of the same type into this one
    fn merge_in(&mut self, model: Self);

    /// Turn this model into a JSON string
    fn stringify(&self) -> TResult<String> {
        jedi::stringify(self).map_err(|e| toterr!(e))
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
        #[allow(unused_imports)]
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
            #[allow(dead_code)]
            pub fn new() -> $name {
                $name {
                    id: None,
                    $( $field: None, )*
                    $( $unserialized: Default::default(), )*
                    _emitter: ::util::event::EventEmitter::new(),
                }
            }

            #[allow(dead_code)]
            pub fn new_with_id() -> $name {
                let mut model = Self::new();
                model.id = match ::models::model::cid() {
                    Ok(x) => Some(x),
                    Err(e) => {
                        error!("model::new() -- problem generating cid: {}", e);
                        None
                    },
                };
                model
            }
        }

        impl ::util::event::Emitter for $name {
            fn bindings(&self) -> &::util::event::Bindings {
                self._emitter.bindings()
            }
        }

        impl ::models::model::Model for $name {
            fn fields(&self) -> Vec<&'static str> {
                vec![ $( stringify!($field) ),* ]
            }

            fn id<'a>(&'a self) -> Option<&'a String> {
                match self.id {
                    Some(ref x) => Some(x),
                    None => None,
                }
            }

            fn merge_in(&mut self, mut model: Self) {
                if model.id.is_some() {
                    self.id = ::std::mem::replace(&mut model.id, None);
                }
                $(
                    if model.$field.is_some() {
                        self.$field = ::std::mem::replace(&mut model.$field, None);
                    }
                )*
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
    use ::jedi::{self, Value};
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

    fn pretest() {
        set_client_id(String::from("c0f4c762af6c42e4079cced2dfe16b4d010b190ad75ade9d83ff8cee0e96586d")).unwrap();
    }

    #[test]
    fn ids() {
        pretest();
        let rabbit = Rabbit::new();
        assert_eq!(rabbit.id, None);
        let rabbit = Rabbit::new_with_id();
        assert!(rabbit.id.is_some());
    }

    #[test]
    fn getter_setter() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.id, None);
        assert_eq!(rabbit.name, None);
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, None);

        rabbit.id = Some(String::from("6969"));
        rabbit.name = Some(String::from("Shredder"));
        rabbit.chews_on_things_that_dont_belong_to_him = Some(true);

        assert_eq!(rabbit.id(), Some(&String::from("6969")));
        assert_eq!(rabbit.id, Some(String::from("6969")));
        assert_eq!(rabbit.name, Some(String::from("Shredder")));
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, Some(true));
    }

    #[test]
    fn reset() {
        let mut rabbit = Rabbit::new();

        rabbit.name = Some(String::from("hoppy"));
        rabbit.city = Some(String::from("santa cruz"));

        let rabbit: Rabbit = jedi::parse(&String::from(r#"{"has_job":false,"name":"slappy","city":"duluth"}"#)).unwrap();

        assert_eq!(rabbit.name, Some(String::from("slappy")));
        assert_eq!(rabbit.city, Some(String::from("duluth")));
    }

    #[test]
    fn stringify() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.stringify().unwrap(), "{\"id\":null,\"name\":null,\"type\":null,\"city\":null,\"chews_on_things_that_dont_belong_to_him\":null}");

        rabbit.id = Some(String::from("12345"));
        rabbit.type_ = Some(String::from("hopper"));
        rabbit.city = Some(String::from("sc"));

        assert_eq!(rabbit.stringify().unwrap(), "{\"id\":\"12345\",\"name\":null,\"type\":\"hopper\",\"city\":\"sc\",\"chews_on_things_that_dont_belong_to_him\":null}");
    }
}


