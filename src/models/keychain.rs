use ::std::collections::HashMap;

use ::error::TResult;
use ::crypto::Key;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::sync::sync_model::{self, MemorySaver};
use ::turtl::TurtlWrap;

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
    match keydata.get(&String::from("s")) {
        Some(x) => return KeyRef::new(x.clone(), String::from("s"), key),
        None => {},
    }
    match keydata.get(&String::from("u")) {
        Some(x) => return KeyRef::new(x.clone(), String::from("u"), key),
        None => {},
    }
    KeyRef::empty()
}

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct KeychainEntry {
        #[serde(rename = "type")]
        #[protected_field(public)]
        pub type_: String,
        #[protected_field(public)]
        pub item_id: String,
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub k: Option<Key>,
    }
}

make_storable!(KeychainEntry, "keychain");
make_basic_sync_model!(KeychainEntry);

#[derive(Debug)]
pub struct Keychain {
    pub entries: Vec<KeychainEntry>,
}

impl Keyfinder for KeychainEntry {}

impl MemorySaver for KeychainEntry {}

impl Keychain {
    /// Create an empty Keychain
    pub fn new() -> Keychain {
        Keychain {
            entries: Vec::new(),
        }
    }

    /// Upsert a key to the keychain
    pub fn upsert_key(&mut self, user_id: &String, item_id: &String, key: &Key, ty: &String, sync_save: Option<TurtlWrap>) -> TResult<()> {
        let remove = {
            let existing = self.find_entry(item_id);
            match existing {
                Some(entry) => {
                    if entry.k.is_some() && entry.k.as_ref().unwrap() == key {
                        return Ok(());
                    }
                    true
                },
                None => false,
            }
        };
        if remove { self.remove_entry(item_id, sync_save.clone())?; }
        let mut entry = KeychainEntry::new();
        entry.type_ = ty.clone();
        entry.user_id = user_id.clone();
        entry.item_id = item_id.clone();
        entry.k = Some(key.clone());
        // if we're saving the model, persist it before adding to the keychain
        match sync_save {
            Some(turtl) => { sync_model::save_model_sync(turtl, &mut entry)?; },
            None => { entry.generate_id()?; },
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Remove a keychain entry
    pub fn remove_entry(&mut self, item_id: &String, sync_save: Option<TurtlWrap>) -> TResult<()> {
        match sync_save {
            Some(turtl) => {
                for entry in &mut self.entries {
                    if &entry.item_id != item_id { continue; }
                    sync_model::delete_model::<KeychainEntry>(turtl.clone(), entry.id().unwrap())?;
                }
            },
            None => {},
        }
        self.entries.retain(|entry| {
            &entry.item_id != item_id
        });
        Ok(())
    }

    /// Find the KeychainEntry matching the given item id
    pub fn find_entry<'a>(&'a self, item_id: &String) -> Option<&'a KeychainEntry> {
        for entry in &self.entries {
            if &entry.item_id == item_id {
                return Some(entry);
            }
        }
        None
    }

    /// Find the key matching a given item id
    pub fn find_key(&self, item_id: &String) -> Option<Key> {
        match self.find_entry(item_id) {
            Some(entry) => {
                if !entry.k.is_some() { return None; }
                Some(entry.k.as_ref().unwrap().clone())
            },
            None => {
                None
            },
        }
    }

    /// Find ALL matching keys for an object.
    pub fn find_all_entries(&self, item_id: &String) -> Vec<Key> {
        let mut found = Vec::with_capacity(2);
        for entry in &self.entries {
            if !entry.k.is_some() { continue; }
            if &entry.item_id == item_id {
                found.push(entry.k.as_ref().unwrap().clone());
            }
        }
        found
    }
}

