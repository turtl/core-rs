use ::std::collections::HashMap;
use ::serde::{ser, de};
use ::error::{TResult, TError};
use ::crypto::Key;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::models::sync_record::{SyncRecord, SyncAction};
use ::models::validate::Validate;
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::turtl::Turtl;
use ::jedi::{self, Value};

/// An enum used to 
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyType {
    #[serde(rename = "s")]
    Space,
    #[serde(rename = "b")]
    Board,
    #[serde(rename = "u")]
    User,
}

impl KeyType {
    pub fn from_string(s: String) -> TResult<Self> {
        let val = Value::String(s);
        Ok(jedi::from_val(val)?)
    }
}

/// Used as an easy object to reference other keys
#[derive(Clone)]
pub struct KeyRef<T: Clone> {
    /// The object id this key is for
    pub id: String,
    /// The object type (s = space, u = user)
    pub ty: KeyType,
    /// encrypted key (Base64-encoded)
    pub k: T,
}

impl<T: Default + Clone> KeyRef<T> {
    /// Create a new keyref
    pub fn new(id: String, ty: KeyType, k: T) -> Self {
        KeyRef {
            id: id,
            ty: ty,
            k: k,
        }
    }
}

impl ser::Serialize for KeyRef<String> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: ser::Serializer
    {
        let mut hash: HashMap<String, String> = HashMap::with_capacity(2);
        let type_str = match jedi::to_val(&self.ty) {
            Ok(x) => {
                match x {
                    Value::String(x) => x,
                    _ => return Err(ser::Error::custom(format!("KeyRef.serialize() -- error stringifying `ty` field"))),
                }
            },
            Err(_) => return Err(ser::Error::custom(format!("KeyRef.serialize() -- error stringifying `ty` field"))),
        };
        hash.insert(type_str, self.id.clone());
        hash.insert(String::from("k"), self.k.clone());
        hash.serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for KeyRef<String> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: de::Deserializer<'de>
    {
        de::Deserialize::deserialize(deserializer)
            .and_then(|mut x: HashMap<String, String>| {
                let key = match x.remove(&String::from("k")) {
                    Some(x) => x,
                    None => return Err(de::Error::invalid_value(de::Unexpected::Map, &"KeyRef.deserialize() -- missing `k` field")),
                };
                let mut keyref: KeyRef<String> = KeyRef::new(String::from(""), KeyType::User, key);
                let typekey = match x.keys().next() {
                    Some(k) => k.clone(),
                    None => return Err(de::Error::invalid_value(de::Unexpected::Map, &"KeyRef.deserialize() -- missing type field")),
                };
                let ty: KeyType = match KeyType::from_string(typekey.clone()) {
                    Ok(x) => x,
                    Err(_) => return Err(de::Error::invalid_value(de::Unexpected::Str(&typekey.as_str()), &"KeyRef.deserialize() -- bad field")),
                };
                let id = x.remove(&typekey).expect("turtl::KeyRef.deserialize() -- could not remove item from hashmap");
                keyref.id = id;
                keyref.ty = ty;
                Ok(keyref)
            })
    }
}

protected! {
    #[derive(Serialize, Deserialize)]
    #[protected_modeltype(keychain)]
    pub struct KeychainEntry {
        #[serde(rename = "type")]
        #[protected_field(public)]
        pub ty: String,
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
impl SyncModel for KeychainEntry {}
impl Keyfinder for KeychainEntry {}
impl Validate for KeychainEntry {}

impl MemorySaver for KeychainEntry {
    fn mem_update(self, turtl: &Turtl, sync_item: &mut SyncRecord) -> TResult<()> {
        let action = sync_item.action.clone();
        match action {
            SyncAction::Add | SyncAction::Edit => {
                if self.k.is_none() {
                    return TErr!(TError::MissingField(String::from("Keychain.k")));
                }
                let mut profile_guard = lockw!(turtl.profile);
                profile_guard.keychain.replace_entry(self)?;
            }
            SyncAction::Delete => {
                let mut profile_guard = lockw!(turtl.profile);
                profile_guard.keychain.remove_entry(&self.item_id, None)?;
            }
            _ => {}
        }
        Ok(())

    }
}

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

    /// Upsert a key to the keychain, don't save
    pub fn upsert_key(&mut self, turtl: &Turtl, item_id: &String, key: &Key, ty: &String) -> TResult<()> {
        let (user_id, user_key) = {
            let user_guard = lockr!(turtl.user);
            let id = user_guard.id_or_else()?;
            let key = user_guard.key_or_else()?;
            (id, key)
        };
        let mut new_entry = KeychainEntry::new();
        let exists = {
            let existing = self.find_entry_mut(item_id);
            let exists = existing.is_some();
            let entry = match existing {
                Some(x) => x,
                None => &mut new_entry,
            };
            entry.set_key(Some(user_key.clone()));
            entry.ty = ty.clone();
            entry.user_id = user_id.clone();
            entry.item_id = item_id.clone();
            entry.k = Some(key.clone());
            entry.generate_id()?;
            exists
        };
        if !exists {
            self.entries.push(new_entry);
        }
        Ok(())
    }

    /// Upsert a keychain entry to the keychain
    pub fn replace_entry(&mut self, entry: KeychainEntry) -> TResult<()> {
        let exists = self.find_entry(&entry.item_id).is_some();
        if exists {
            self.remove_entry(&entry.item_id, None)?;
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Remove a keychain entry
    pub fn remove_entry(&mut self, item_id: &String, sync_save: Option<(&Turtl, bool)>) -> TResult<()> {
        match sync_save {
            Some((turtl, skip_remote_sync)) => {
                for entry in &mut self.entries {
                    if &entry.item_id != item_id { continue; }
                    sync_model::delete_model::<KeychainEntry>(turtl, entry.id().expect("turtl::Keychain.remove_entry() -- entry.id() is None"), skip_remote_sync)?;
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
    pub fn find_entry_mut<'a>(&'a mut self, item_id: &String) -> Option<&'a mut KeychainEntry> {
        for entry in &mut self.entries {
            if &entry.item_id == item_id {
                return Some(entry);
            }
        }
        None
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
                Some(entry.k.as_ref().expect("turtl::Keychain::find_key() -- entry.k is None HOW CAN THIS BE").clone())
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
                found.push(entry.k.as_ref().expect("turtl::Keychain::find_all_entries() -- entry.k is NONE OMG HELLLPPP").clone());
            }
        }
        found
    }
}

// NOTE: for the following two functions, instead of saving to
// Turtl.profile.keychain directly, we copy/clone data out of the real keychain.
//
// if we don't do it this way, then we have a write lock on
// Turtl.profile when the MemorySaver runs for the new key and we get
// a deadlock on any model that uses add_to_keychain() == true.
//
// so this, might seem a bit roundabout, but it lets us use MemorySaver
// for adding keys to the in-mem keychain so that logic can live in just
// one place.
//
// also keep in mind, there are a few places where we circumvent deadlocks by
// triggering app events that run outside of the current thread, however we
// DON'T want to do that here because keys should *always* be managed internally
// by the core and shouldn't be sent out (even to the UI). since the UI has the
// ability to listen on any messaging channel, we avoid sending keydata using
// this method.
// >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
/// Save a key to the keychain for the current logged in user
pub fn save_key(turtl: &Turtl, item_id: &String, key: &Key, ty: &String, skip_remote_sync: bool) -> TResult<()> {
    let (user_id, user_key) = {
        let user_guard = lockr!(turtl.user);
        let id = user_guard.id_or_else()?;
        let key = user_guard.key_or_else()?;
        (id, key)
    };

    let mut new_entry = KeychainEntry::new();
    let mut existing: Option<KeychainEntry> = {
        let profile_guard = lockr!(turtl.profile);
        match profile_guard.keychain.find_entry(item_id) {
            Some(x) => Some(x.clone()?),
            None => None,
        }
    };
    let exists = existing.is_some();
    let entry = match existing.as_mut() {
        Some(x) => x,
        None => &mut new_entry,
    };

    entry.set_key(Some(user_key.clone()));
    entry.ty = ty.clone();
    entry.user_id = user_id.clone();
    entry.item_id = item_id.clone();
    entry.k = Some(key.clone());

    let action = if exists { SyncAction::Edit } else { SyncAction::Add };
    sync_model::save_model(action, turtl, entry, skip_remote_sync)?;
    Ok(())
}

/// Remove a key from the keychain for the current logged in user
pub fn remove_key(turtl: &Turtl, item_id: &String, skip_remote_sync: bool) -> TResult<()> {
    let entry_ids = {
        let profile_guard = lockr!(turtl.profile);
        let mut ids = Vec::new();
        for entry in &profile_guard.keychain.entries {
            if &entry.item_id == item_id {
                ids.push(entry.id().expect("turtl::keychain::remove_key() -- entry.id() is None nooo").clone());
            }
        }
        ids
    };
    for entry_id in entry_ids {
        sync_model::delete_model::<KeychainEntry>(turtl, &entry_id, skip_remote_sync)?;
    }
    Ok(())
}
// <<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<<

#[cfg(test)]
pub mod tests {
    use super::*;
    use ::crypto::Key;

    #[test]
    fn upserts_keys_properly() {
        let turtl = ::turtl::tests::with_test(true);
        let mut kc = Keychain::new();
        let key1 = Key::random().unwrap();
        let mut key2 = Key::random().unwrap();
        let item1_id = String::from("1234");
        let ty = String::from("space");
        loop {
            if key1.data() != key2.data() { break; }
            key2 = Key::random().unwrap();
        }
        kc.upsert_key(&turtl, &item1_id, &key1, &ty).unwrap();
        let entry_a_id = kc.find_entry(&item1_id).unwrap().id().unwrap().clone();
        kc.upsert_key(&turtl, &item1_id, &key2, &ty).unwrap();
        let entry_b_id = kc.find_entry(&item1_id).unwrap().id().unwrap().clone();
        assert_eq!(entry_a_id, entry_b_id);
    }
}

