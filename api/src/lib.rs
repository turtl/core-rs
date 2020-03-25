//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

#[macro_use]
mod util;

#[cfg(feature = "wasm")]
mod api_wasm;
#[cfg(feature = "wasm")]
pub use api_wasm::*;

#[cfg(not(feature = "wasm"))]
mod api_reqwest;
#[cfg(not(feature = "wasm"))]
pub use api_reqwest::*;

