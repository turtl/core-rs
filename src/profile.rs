//! The Profile module exports a struct that is responsible for handling the
//! user's data (keychain, boards, etc).
//!
//! It only stores data for the keychain, persona (soon deprecated), and boards
//! (so no note data). The reason is that keychain/boards are useful to keep in
//! memory to decrypt notes, but otherwise, notes can just be loaded on the fly
//! from local storage and discarded once sent to the UI.

use ::models::keychain::Keychain;
use ::models::persona::Persona;
use ::models::board::Board;

pub struct Profile {
    pub keychain: Vec<Keychain>,
    pub boards: Vec<Board>,
    pub persona: Option<Persona>,
}

impl Profile {
    pub fn new() -> Profile {
        Profile {
            keychain: Vec::new(),
            boards: Vec::new(),
            persona: None,
        }
    }
}

