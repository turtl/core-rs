use ::std::collections::HashMap;

use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

/// Used as an easy object to reference other keys
pub struct KeyRef<T> {
    /// The object id this key is for
    pub id: String,
    /// The object type (b = board, u = user, p = persona)
    pub ty: String,
    /// encrypted key (Base64-encoded)
    pub k: T,
}

impl<T: Default> KeyRef<T> {
    /// Create a new key search entry
    pub fn new(id: String, ty: String, k: T) -> KeyRef<T> {
        KeyRef {
            id: id,
            ty: ty,
            k: k,
        }
    }

    /// Create a new EMPTY key search entry
    pub fn empty() -> KeyRef<T> {
        KeyRef {
            id: String::from(""),
            ty: String::from(""),
            k: Default::default(),
        }
    }
}

/// Takes some key data from a protected model and turns it into a
/// KeyRef
pub fn keyref_from_encrypted(keydata: &HashMap<String, String>) -> KeyRef<String> {
    let key = match keydata.get(&String::from("k")) {
        Some(x) => x.clone(),
        None => return KeyRef::empty(),
    };

    match keydata.get(&String::from("b")) {
        Some(x) => return KeyRef::new(x.clone(), String::from("b"), key),
        None => {},
    }
    match keydata.get(&String::from("p")) {
        Some(x) => return KeyRef::new(x.clone(), String::from("p"), key),
        None => {},
    }
    match keydata.get(&String::from("u")) {
        Some(x) => return KeyRef::new(x.clone(), String::from("u"), key),
        None => {},
    }
    KeyRef::empty()
}

protected!{
    pub struct KeychainEntry {
        ( type_: String,
          item_id: String,
          user_id: String ),
        ( k: Vec<u8> ),
        ( )
    }
}

make_basic_sync_model!(KeychainEntry);

pub struct Keychain {
    pub entries: Vec<KeychainEntry>,
}

impl Keyfinder for KeychainEntry {}

impl Keychain {
    /// Create an empty Keychain
    pub fn new() -> Keychain {
        Keychain {
            entries: Vec::new(),
        }
    }

    /// Add a key to the keychain
    pub fn add_key(&mut self, user_id: &String, item_id: &String, key: &Vec<u8>, ty: &String) {
        let mut entry = KeychainEntry::new();
        entry.type_ = Some(ty.clone());
        entry.user_id = Some(user_id.clone());
        entry.item_id = Some(item_id.clone());
        entry.k = Some(key.clone());
        self.entries.push(entry);
    }

    /// Find the key matching a given item id
    pub fn find_entry(&self, item_id: &String) -> Option<Vec<u8>> {
        for entry in &self.entries {
            if !entry.item_id.is_some() || !entry.k.is_some() { continue; }
            let entry_item_id = entry.item_id.as_ref().unwrap();
            if entry_item_id == item_id {
                return Some(entry.k.as_ref().unwrap().clone());
            }
        }
        None
    }

    /// Find ALL matching keys for an object.
    pub fn find_all_entries(&self, item_id: &String) -> Vec<Vec<u8>> {
        let mut found = Vec::with_capacity(2);
        for entry in &self.entries {
            if !entry.item_id.is_some() || !entry.k.is_some() { continue; }
            let entry_item_id = entry.item_id.as_ref().unwrap();
            if entry_item_id == item_id {
                found.push(entry.k.as_ref().unwrap().clone());
            }
        }
        found
    }
}

