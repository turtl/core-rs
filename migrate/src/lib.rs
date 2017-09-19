#[macro_use]
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

mod api;
mod error;
mod crypto;
mod user;

use api::{Api, ApiReq};

/// Check if an account exists on the old server
pub fn check_login(username: &String, password: &String) -> MResult<bool> {
    let api = Api::new();
    Ok(true)
}

