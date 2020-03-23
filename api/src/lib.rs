//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

extern crate base64;
extern crate config;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate quick_error;
extern crate reqwest;

#[cfg(feature = "wasm")]
mod api_wasm;
#[cfg(feature = "wasm")]
pub use api_wasm::*;

#[cfg(not(feature = "wasm"))]
mod api_reqwest;
#[cfg(not(feature = "wasm"))]
pub use api_reqwest::*;

