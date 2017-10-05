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
pub use crypto::Key;

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

fn get_profile(auth: &String) -> MResult<Vec<SyncRecord>> {
    let mut api = Api::new();
    api.set_auth(auth.clone())?;
    let syncdata: SyncResponse = api.get("/sync/full", ApiReq::new().timeout(120))?;
    let SyncResponse { records, .. } = syncdata;
    Ok(records)
}

fn decrypt_profile(profile: Vec<SyncRecord>) -> MResult<()> {
    Ok(())
}

/// Holds login info.
pub struct Login {
    auth: String,
    key: Key,
}

impl Login {
    fn new(auth: String, key: Key) -> Self {
        Login {
            auth: auth,
            key: key,
        }
    }
}

/// Check if an account exists on the old server
pub fn check_login(username: &String, password: &String) -> MResult<Option<Login>> {
    let mut api = Api::new();
    let (key1, auth1) = user::generate_auth(username, password, 1)?;
    api.set_auth(auth1.clone())?;
    match api.post::<String>("/auth", ApiReq::new()) {
        Ok(_) => { return Ok(Some(Login::new(auth1, key1))); }
        Err(_) => {}
    }
    let (key0, auth0) = user::generate_auth(username, password, 0)?;
    api.set_auth(auth0.clone())?;
    match api.post::<String>("/auth", ApiReq::new()) {
        Ok(_) => { return Ok(Some(Login::new(auth0, key0))); }
        Err(_) => {}
    }
    Ok(None)
}

/// Holds filedata.
pub struct File {
    note_id: String,
    data: Vec<u8>,
}

/// Holds the result of a profile migration.
pub struct MigrateResult {
    spaces: Vec<Value>,
    boards: Vec<Value>,
    notes: Vec<Value>,
    files: Vec<File>,
}

/// Migrate a v6 account to a v7 account. We do this by creating sync items
pub fn migrate(v6_auth: &String) -> MResult<MigrateResult> {
    let profile = get_profile(v6_auth)?;
    let decrypted = decrypt_profile(profile)?;
    Ok(MigrateResult {
        spaces: Vec::new(),
        boards: Vec::new(),
        notes: Vec::new(),
        files: Vec::new(),
    })
}

