extern crate config;
extern crate crypto as rust_crypto;
extern crate fern;
extern crate gcrypt;
extern crate hyper;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate rustc_serialize as serialize;
extern crate serde;
#[macro_use]
extern crate serde_derive;

#[macro_use]
mod error;
mod api;
mod crypto;
mod user;

use ::api::{Api, ApiReq};
use ::error::MResult;
use ::jedi::Value;

/// Check if an account exists on the old server
pub fn check_login(username: &String, password: &String) -> MResult<Option<String>> {
    let mut api = Api::new();
    let (_, auth1) = user::generate_auth(username, password, 1)?;
    api.set_auth(auth1.clone())?;
    match api.post::<String>("/auth", ApiReq::new().timeout(30)) {
        Ok(_) => { return Ok(Some(auth1)); }
        Err(_) => {}
    }
    let (_, auth0) = user::generate_auth(username, password, 0)?;
    api.set_auth(auth0.clone())?;
    match api.post::<String>("/auth", ApiReq::new().timeout(30)) {
        Ok(_) => { return Ok(Some(auth0)); }
        Err(_) => {}
    }
    Ok(None)
}

/// A shitty placeholder for a sync record
#[derive(Deserialize, Debug)]
struct SyncRecord {
    #[serde(rename = "type")]
    pub ty: String,
    pub data: Option<Value>,
}

/// Defines a struct for deserializing our incoming sync response
#[derive(Deserialize, Debug)]
struct SyncResponse {
    #[serde(default)]
    records: Vec<SyncRecord>,
    #[serde(default)]
    sync_id: String,
}

pub fn get_profile(auth: &String) -> MResult<()> {
    let mut api = Api::new();
    api.set_auth(auth.clone())?;
    let syncdata: SyncResponse = api.get("/sync/full", ApiReq::new().timeout(120))?;
    Ok(())
}

