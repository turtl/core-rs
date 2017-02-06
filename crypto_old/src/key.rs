//! This submodule defines a a cryptographic key

use ::serde::{ser, de};

use ::CResult;

/// A type we'll use to represent crypto keys
#[derive(Debug)]
pub struct Key {
    /// Holds the actual bytes for our key
    data: Vec<u8>,
}

impl Key {
    /// Create a new key from some keydata
    pub fn new(data: Vec<u8>) -> Key {
        Key {
            data: data,
        }
    }

    /// Create a new random key
    pub fn random() -> CResult<Key> {
        Ok(Key::new(::low::rand_bytes(32)?))
    }

    /// Return a ref to this key's data
    pub fn data<'a>(&'a self) -> &'a Vec<u8> {
        &self.data
    }

    /// Consume this Key and convert it into its underlying data
    #[allow(dead_code)]
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// Return this key's data length
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl Clone for Key {
    fn clone(&self) -> Key {
        Key::new(self.data().clone())
    }
}

impl Default for Key {
    fn default() -> Key {
        Key::new(Vec::new())
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        self.data() == other.data()
    }
}

impl ser::Serialize for Key {
    fn serialize<S>(&self, serializer:S) -> Result<S::Ok, S::Error>
        where S: ser::Serializer
    {
        let base64: String = match ::to_base64(self.data()) {
            Ok(x) => x,
            Err(e) => return Err(ser::Error::custom(format!("Key.serialize() -- error converting to base64: {}", e))),
        };
        base64.serialize(serializer)
    }
}

impl de::Deserialize for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: de::Deserializer
    {
        de::Deserialize::deserialize(deserializer)
            .and_then(|x| {
                match ::from_base64(&x) {
                    Ok(x) => Ok(Key::new(x)),
                    Err(_) => return Err(de::Error::invalid_value(de::Unexpected::Str(&x.as_str()), &"Key.deserialize() -- invalid base64")),
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi;

    #[test]
    fn openparen_de_closeparen_serializes() {
        let keys: Vec<Key> = jedi::parse(&String::from(r#"["XExP/+h80Fm06fEqKsKoE5GwaDRY88pObH+y6YCTWzQ="]"#)).unwrap();
        let key: Key = jedi::parse(&String::from(r#""XExP/+h80Fm06fEqKsKoE5GwaDRY88pObH+y6YCTWzQ=""#)).unwrap();
        let des_key: Vec<u8> = vec![92, 76, 79, 255, 232, 124, 208, 89, 180, 233, 241, 42, 42, 194, 168, 19, 145, 176, 104, 52, 88, 243, 202, 78, 108, 127, 178, 233, 128, 147, 91, 52];
        assert_eq!(keys[0].data(), &des_key);
        assert_eq!(key.data(), &des_key);

        let ser_key = jedi::stringify(&key).unwrap();
        assert_eq!(ser_key, String::from(r#""XExP/+h80Fm06fEqKsKoE5GwaDRY88pObH+y6YCTWzQ=""#));
    }
}

