//! The `Model` module defines a container for data, and also interfaces for
//! syncing said data to local databases.

#[macro_use]
pub mod model;
#[macro_use]
pub mod protected;
#[macro_use]
pub mod storable;

pub mod user;
pub mod keychain;
pub mod space;
pub mod board;
pub mod note;
pub mod file;
pub mod invite;

