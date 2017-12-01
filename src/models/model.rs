//! The Model type defines an object that maps user's data (a note, a board,
//! etc etc) to a database table and/or a set of methods/operations that can be
//! run on that data.
//!
//! The most important aspect of models is that they are (De)Serialize(able),
//! making them easy to save/load to various data sources.

use ::std::sync::RwLock;

use ::time;
use ::serde::ser::Serialize;
use ::serde::de::DeserializeOwned;
use ::jedi::{self, Value};
use ::crypto;
use ::error::{TError, TResult};

lazy_static! {
    /// create a static/global cid counter
    static ref CID_COUNTER: RwLock<u32> = RwLock::new(0);

    /// holds our app's client id
    static ref CLIENT_ID: RwLock<Option<String>> = RwLock::new(None);
}

/// A macro that makes it easy to create one-off Option field grabbers for model
/// fields.
///
/// Example:
///
///   model_getter!(get_field, "Search.index_note()");
///   let id = get_field!(mymodel, id);
///   let name = get_field!(mymodel, name, String::from("default name"));
#[macro_export]
macro_rules! model_getter {
    ($name:ident, $func:expr) => {
        macro_rules! $name {
            // this variant throws an enourmous tantrum of epic proportions if
            // the model field is None
            ($model:ident, $field:ident) => {
                match $model.$field.as_ref() {
                    Some(val) => val.clone(),
                    None => return TErr!(::error::TError::MissingField(format!("{}", stringify!($field)))),
                }
            };

            // this variant returns a default value if the model field is None
            ($model:ident, $field:ident, $def:expr) => {
                match $model.$field.as_ref() {
                    Some(val) => val.clone(),
                    None => $def,
                }
            };
        }
    }
}

/// Set the model system's client id
pub fn get_client_id() -> Option<String> {
    let guard = lockr!((*CLIENT_ID));
    (*guard).clone()
}

/// Set the model system's client id
pub fn set_client_id(id: String) -> TResult<()> {
    debug!("model -- set_client_id(): {}", id);
    let mut guard = lockw!((*CLIENT_ID));
    *guard = Some(id);
    Ok(())
}

/// Create a turtl object id from a client id
pub fn cid() -> TResult<String> {
    let client_id = match get_client_id() {
        Some(ref x) => x.clone(),
        None => return TErr!(TError::MissingData(format!("CLIENT_ID missing"))),
    };
    let mut counter_guard = lockw!((*CID_COUNTER));
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

/// Given a cid and a client id, replace the cid's client id with the given one.
pub fn cid_w_client_id(cid: &String, client_id: &String) -> TResult<String> {
    let mut cid_bytes = crypto::from_hex(cid)?;
    let client_id_bytes = crypto::from_hex(client_id)?;
    for i in 0..32 {
        cid_bytes[i + 6] = client_id_bytes[i];
    }
    Ok(crypto::to_hex(&cid_bytes)?)
}

/// Parse a unix timestamp out of a model id
pub fn id_timestamp(id: &String) -> TResult<i64> {
    let ts = if id.len() == 24 {
        i64::from_str_radix(&id[0..8], 16)? * 1000
    } else if id.len() == 80 {
        i64::from_str_radix(&id[0..12], 16)?
    } else {
        return TErr!(TError::BadValue(format!("bad id given ({})", id)));
    };
    Ok(ts)
}

/// The model trait defines an interface for (de)serializable objects that track
/// their changes via eventing.
pub trait Model: Serialize + DeserializeOwned + Default {
    /// Get this model's ID
    fn id<'a>(&'a self) -> Option<&'a String>;

    /// Set this model's ID
    fn set_id<'a>(&mut self, id: String);

    /// Generate an id for this model if it doesn't have one
    fn generate_id<'a>(&'a mut self) -> TResult<&'a String>;

    /// Return a result to this models id. Ok if it exists, error if None
    fn id_or_else(&self) -> TResult<String>;

    /// Turn this model into a JSON string
    fn stringify(&self) -> TResult<String> {
        jedi::stringify(self).map_err(|e| toterr!(e))
    }

    /// Create a new model from a JSON dump.
    fn clone_from(data: Value) -> TResult<Self> {
        jedi::from_val(data).map_err(|e| toterr!(e))
    }

    /// Determine if this model has been saved already or not
    fn is_new(&self) -> bool {
        self.id().is_none()
    }

    /// Return a reference to this model. Useful in cases where the model is
    /// wrapped in a container (RwLock, et al) and you need a ref to it.
    fn as_ref<'a>(&self) -> &Self {
        self
    }

    /// Return a mutable reference to this model. Useful in cases where the
    /// model is wrapped in a container (RwLock, et al) and you need a ref to
    /// it.
    fn as_mut<'a>(&'a mut self) -> &'a mut Self {
        self
    }
}

#[macro_export]
/// Defines a model. Adds a few fields to a struct def that models user, and
/// runs some simple impls for us.
macro_rules! model {
    (
        $(#[$struct_meta:meta])*
        pub struct $name:ident {
            $( $inner:tt )*
        }
    ) => {
        $(#[$struct_meta])*
        #[derive(Default)]
        pub struct $name {
            #[serde(default)]
            #[serde(skip_serializing_if = "Option::is_none")]
            #[serde(deserialize_with = "::util::ser::int_opt_converter::deserialize")]
            pub id: Option<String>,
            $( $inner )*
        }

        impl $name {
            #[allow(dead_code)]
            pub fn new() -> Self {
                Default::default()
            }

            #[allow(dead_code)]
            pub fn new_with_id() -> ::error::TResult<$name> {
                let mut model = Self::new();
                model.id = Some(::models::model::cid()?);
                Ok(model)
            }
        }

        impl ::models::model::Model for $name {
            fn id<'a>(&'a self) -> Option<&'a String> {
                match self.id {
                    Some(ref x) => Some(x),
                    None => None,
                }
            }

            fn id_or_else(&self) -> ::error::TResult<String> {
                match self.id() {
                    Some(id) => Ok(id.clone()),
                    None => TErr!(::error::TError::MissingField(format!("{}.id", stringify!($name)))),
                }
            }

            fn set_id(&mut self, id: String) {
                self.id = Some(id);
            }

            fn generate_id<'a>(&'a mut self) -> ::error::TResult<&'a String> {
                if self.id.is_none() {
                    self.id = Some(::models::model::cid()?);
                }
                Ok(self.id.as_ref().unwrap())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi::{self, Value};

    use ::error::TResult;

    model! {
        #[derive(Debug, Serialize, Deserialize)]
        pub struct Rabbit {
            name: Option<String>,
            #[serde(rename = "type")]
            type_: Option<String>,
            city: Option<String>,
            chews_on_things_that_dont_belong_to_him: Option<bool>,
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
        let rabbit = Rabbit::new_with_id().unwrap();
        assert!(rabbit.id.is_some());
    }

    #[test]
    fn blank() {
        let rabbit = Rabbit::new();
        assert_eq!(rabbit.id, None);
        assert_eq!(rabbit.name, None);
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, None);
    }

    #[test]
    fn reset() {
        let rabbit: Rabbit = jedi::parse(&String::from(r#"{"id":"17"}"#)).unwrap();
        assert_eq!(rabbit.id, Some(String::from("17")));
        assert_eq!(rabbit.name, None);
        assert_eq!(rabbit.chews_on_things_that_dont_belong_to_him, None);

        let mut rabbit = Rabbit::new();
        rabbit.id = None;
        rabbit.name = Some(String::from("hoppy"));
        rabbit.city = Some(String::from("santa cruz"));

        let val: Value = jedi::parse(&String::from(r#"{"id":"6969","name":"slappy","city":"duluth"}"#)).unwrap();
        let rabbit2: Rabbit = Rabbit::clone_from(val).unwrap();

        assert_eq!(rabbit2.id, Some(String::from("6969")));
        assert_eq!(rabbit2.name, Some(String::from("slappy")));
        assert_eq!(rabbit2.city, Some(String::from("duluth")));
    }

    #[test]
    fn stringify() {
        let mut rabbit = Rabbit::new();
        assert_eq!(rabbit.stringify().unwrap(), "{\"name\":null,\"type\":null,\"city\":null,\"chews_on_things_that_dont_belong_to_him\":null}");

        rabbit.id = Some(String::from("12345"));
        rabbit.type_ = Some(String::from("hopper"));
        rabbit.city = Some(String::from("sc"));

        assert_eq!(rabbit.stringify().unwrap(), "{\"id\":\"12345\",\"name\":null,\"type\":\"hopper\",\"city\":\"sc\",\"chews_on_things_that_dont_belong_to_him\":null}");
    }

    #[test]
    fn model_getter() {
        model_getter!(get_val, "model_getter.test()");
        fn run_test1(rabbit: &Rabbit) -> TResult<()> {
            assert_eq!(get_val!(rabbit, id), "omglolwtf");
            assert_eq!(get_val!(rabbit, name), "flirty");
            assert_eq!(get_val!(rabbit, type_), "dutch");
            assert_eq!(get_val!(rabbit, city, String::from("santa cruz")), "santa cruz");
            Ok(())
        }
        fn run_test2(rabbit: &Rabbit) -> TResult<()> {
            get_val!(rabbit, city);
            Ok(())
        }

        let rabbit: Rabbit = jedi::parse(&String::from(r#"{"id":"omglolwtf","name":"flirty","type":"dutch"}"#)).unwrap();
        assert!(run_test1(&rabbit).is_ok());
        assert!(run_test2(&rabbit).is_err());
    }
}


