use ::std::collections::HashMap;

use ::error::{TResult, TError};
use ::crypto::Key;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::sync::sync_model::{self, MemorySaver};
use ::turtl::Turtl;

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

impl Keyfinder for KeychainEntry {}
impl MemorySaver for KeychainEntry {}

#[derive(Debug)]
pub struct Keychain {
    pub entries: Vec<KeychainEntry>,
}

impl Keychain {
    /// Create an empty Keychain
    pub fn new() -> Keychain {
        Keychain {
            entries: Vec::new(),
        }
    }

    /// Upsert a key to the keychain
    fn upsert_key_impl(&mut self, turtl: &Turtl, item_id: &String, key: &Key, ty: &String, save: bool, skip_remote_sync: bool) -> TResult<()> {
        let (user_id, user_key) = {
            let user_guard = turtl.user.read().unwrap();
            let id = match user_guard.id() {
                Some(id) => id.clone(),
                None => return Err(TError::MissingField(String::from("Keychain.upsert_key_save() -- `turtl.user` is missing an id. harrr."))),
            };
            let key = match user_guard.key() {
                Some(k) => k.clone(),
                None => return Err(TError::MissingField(String::from("Keychain.upsert_key_save() -- `turtl.user` is missing a key. gfft."))),
            };
            (id, key)
        };
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
        if save && remove {
            self.remove_entry(item_id, Some((turtl, skip_remote_sync)))?;
        }
        let mut entry = KeychainEntry::new();
        entry.set_key(Some(user_key.clone()));
        entry.type_ = ty.clone();
        entry.user_id = user_id.clone();
        entry.item_id = item_id.clone();
        entry.k = Some(key.clone());
        if save {
            sync_model::save_model(&String::from("create"), turtl, &mut entry, skip_remote_sync)?;
        } else {
            entry.generate_id()?;
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Upsert a key to the keychain, don't save
    pub fn upsert_key(&mut self, turtl: &Turtl, item_id: &String, key: &Key, ty: &String) -> TResult<()> {
        self.upsert_key_impl(turtl, item_id, key, ty, false, true)
    }

    /// Upsert a key to the keychain, then save (sync)
    pub fn upsert_key_save(&mut self, turtl: &Turtl, item_id: &String, key: &Key, ty: &String, skip_remote_sync: bool) -> TResult<()> {
        self.upsert_key_impl(turtl, item_id, key, ty, true, skip_remote_sync)
    }

    /// Remove a keychain entry
    pub fn remove_entry(&mut self, item_id: &String, sync_save: Option<(&Turtl, bool)>) -> TResult<()> {
        match sync_save {
            Some((turtl, skip_remote_sync)) => {
                for entry in &mut self.entries {
                    if &entry.item_id != item_id { continue; }
                    sync_model::delete_model::<KeychainEntry>(turtl, entry.id().unwrap(), skip_remote_sync)?;
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

