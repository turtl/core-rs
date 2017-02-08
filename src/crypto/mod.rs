/// Turtl's crypto module. Provides a standard interface for all things crpyto
/// related.

mod error;
mod low;
mod key;

pub use ::crypto::error::{
    CResult,
    CryptoError,
};
pub use ::crypto::low::{
    sha256,
    sha512,
    to_hex,
    from_hex,
    to_base64,
    from_base64,
    KEYGEN_SALT_LEN,
    KEYGEN_OPS_DEFAULT,
    KEYGEN_MEM_DEFAULT,
    random_salt,
};
pub use ::crypto::low::chacha20poly1305::{random_nonce, random_key, noncelen, keylen};
pub use ::crypto::key::Key;

/// Stores our current crypto version. This gets encoded into a header in the
/// ciphertext and lets the crypto module know how to handle the message.
const CRYPTO_VERSION: u16 = 6;

/// Stores the available algorithms for symmetric crypto.
const SYM_ALGORITHM: [&'static str; 1] = ["chacha20poly1305"];

/// Find the position of a static string in an array of static strings
fn find_index(arr: &[&'static str], val: &str) -> CResult<usize> {
    for i in 0..arr.len() {
        if val == arr[i] { return Ok(i) }
    }
    Err(CryptoError::Msg(format!("not found: {}", val)))
}

/// Describes how we want to run our encryption.
#[derive(Debug)]
pub struct CryptoOp {
    algorithm: &'static str,
    nonce: Option<Vec<u8>>,
}
impl CryptoOp {
    /// Create a new crypto op with a cipher/blockmode
    pub fn new(algorithm: &'static str) -> CResult<CryptoOp> {
        find_index(&SYM_ALGORITHM, algorithm)?;
        Ok(CryptoOp { algorithm: algorithm, nonce: None })
    }

    /// Create a new crypto op with a algorith/nonce
    #[allow(dead_code)]
    pub fn new_with_iv(algorithm: &'static str, nonce: Vec<u8>) -> CResult<CryptoOp> {
        let mut op = CryptoOp::new(algorithm)?;
        op.nonce = Some(nonce);
        Ok(op)
    }
}

/// Describes some meta about our payload. This includes the version
/// algorithm used.
#[derive(Debug)]
pub struct PayloadDescription {
    pub algorithm: u8,
}
impl PayloadDescription {
    /// Create a new PayloadDescription from a crypto version and some other data
    pub fn new(_crypto_version: u16, algorithm: &str) -> CResult<PayloadDescription> {
        let desc: Vec<u8> = vec![find_index(&SYM_ALGORITHM, algorithm)? as u8];
        Ok(PayloadDescription::from(desc.as_slice())?)
    }

    /// Convert a byte vector into a PayloadDescription object
    pub fn from(data: &[u8]) -> CResult<PayloadDescription> {
        if data.len() < 1 { return Err(CryptoError::Msg(format!("PayloadDescription::from - bad desc length (< 2)"))) }
        let algorithm = data[0];
        Ok(PayloadDescription {
            algorithm: algorithm,
        })
    }

    /// Covert this payload description into a byte vector
    pub fn as_vec(&self) -> Vec<u8> {
        vec![self.algorithm]
    }

    /// Get the payload description's size
    pub fn len(&self) -> usize {
        self.as_vec().len()
    }
}

/// Holds a deserialized description of our ciphertext.
#[derive(Debug)]
pub struct CryptoData {
    pub version: u16,
    pub desc: PayloadDescription,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl CryptoData {
    /// Create a new CryptoData object from its parts
    pub fn new(version: u16, desc: PayloadDescription, nonce: Vec<u8>, ciphertext: Vec<u8>) -> CryptoData {
        CryptoData {
            version: version,
            desc: desc,
            nonce: nonce,
            ciphertext: ciphertext,
        }
    }

    /// Returns the estimated serialized size of this CryptoData
    pub fn len(&self) -> usize {
        let version_size: usize = 2;
        return (version_size + self.desc.len() + self.nonce.len() + self.ciphertext.len()) as usize;
    }
}

/// Deserialize a serialized cryptographic message. Basically, each piece of
/// crypto data in Turtl has a header, followed by N bytes of ciphertext in
/// the following format:
///
///   |-2 bytes-| |-1 byte----| |-N bytes-----------| |-1 byte-----| |-N bytes-| |-N bytes--|
///   | version | |desc length| |payload description| |nonce length| |  nonce  | |ciphertext|
///
/// - `version` tells us the serialization version. although it will probably
///   not get over 255, it has two bytes just in case. never say never.
/// - `desc length` is the length of the payload description, which may change
///   in length from version to version.
/// - `payload description` tells us what algorithm/format the encryption uses.
///   This holds 1-byte array indexes for our crypto values (SYM_ALGORITHM),
///   which tells us the cipher, block mode, etc (and how to decrypt this data).
/// - `nonce length` is the length of the nonce
/// - `nonce` is the initial vector of the payload.
/// - `ciphertext` is our actual encrypted data. DUUUuuuUUUHHH
///
pub fn deserialize(mut serialized: Vec<u8>) -> CResult<CryptoData> {
    let mut idx: usize = 0;
    let version: u16 = ((serialized[idx] as u16) << 8) + (serialized[idx + 1] as u16);
    idx += 2;

    let desc_struct = {
        let desc_length = serialized[idx];
        idx += 1;
        let desc = &serialized[idx..(idx + (desc_length as usize))];
        idx += desc_length as usize;
        PayloadDescription::from(desc)?
    };

    let nonce_length = serialized[idx] as usize;
    idx += 1;
    let nonce = Vec::from(&serialized[idx..(idx + nonce_length)]);
    idx += nonce_length;

    // non-copying conversion into a vec
    let ciphertext: Vec<u8> = serialized.drain(idx..).collect();

    Ok(CryptoData::new(version, desc_struct, nonce, ciphertext))
}

/// Serialize a CryptoData container into a raw header vector. This is useful
/// for extracting authentication data.
pub fn serialize_header(data: &CryptoData) -> CResult<Vec<u8>> {
    let mut ser: Vec<u8> = Vec::with_capacity(data.len());

    // add the two-byte version
    ser.push((data.version >> 8) as u8);
    ser.push((data.version & 0xFF) as u8);

    // add description length and description
    ser.push(data.desc.len() as u8);
    ser.append(&mut data.desc.as_vec().clone());

    // add the nonce
    ser.push(data.nonce.len() as u8);
    ser.append(&mut data.nonce.clone());

    Ok(ser)
}

/// Serialize a CryptoData container into a vector of bytes. For more info on
/// the different serialization formats, check out deserialize().
pub fn serialize(data: &mut CryptoData) -> CResult<Vec<u8>> {
    // grab the raw header
    let mut ser = serialize_header(data)?;

    // lastly, our ciphertext
    ser.append(&mut data.ciphertext);

    Ok(ser)
}

/// Decrypt a message, given a key and the ciphertext. The ciphertext should
/// contain *all* the data needed to decrypt the message encoded in a header
/// (see deserialize() for more info).
pub fn decrypt(key: &Key, ciphertext: Vec<u8>) -> CResult<Vec<u8>> {
    let deserialized = deserialize(ciphertext)?;
    let desc = &deserialized.desc;
    let nonce = &deserialized.nonce;
    let ciphertext = &deserialized.ciphertext;
    let auth: Vec<u8> = serialize_header(&deserialized)?;
    let decrypted = match SYM_ALGORITHM[desc.algorithm as usize] {
        "chacha20poly1305" => {
            low::chacha20poly1305::decrypt(key.data().as_slice(), nonce.as_slice(), auth.as_slice(), ciphertext.as_slice())?
        },
        _ => {
            return Err(CryptoError::NotImplemented(format!("the algorithm in this payload was not found: {}", desc.algorithm)));
        }
    };
    Ok(decrypted)
}

/// Encrypt a message, given a key and the plaintext. This returns the
/// ciphertext serialized via Turtl serialization format (see deserialize() for
/// more info).
///
/// Note that this function *ONLY* supports encrypting the current crypto
/// version (CRYPTO_VERSION). The idea is that later versions are most likely
/// more secure or correct than earlier versions, so we just don't allow going
/// back in time (although decrypt() supports all previous versions).
pub fn encrypt(key: &Key, plaintext: Vec<u8>, op: CryptoOp) -> CResult<Vec<u8>> {
    let version = CRYPTO_VERSION;
    match op.algorithm {
        "chacha20poly1305" => {
            let nonce = match op.nonce {
                Some(x) => x,
                None => low::chacha20poly1305::random_nonce()?,
            };
            let desc = PayloadDescription::new(version, op.algorithm)?;
            let mut data = CryptoData::new(version, desc, nonce, Vec::new());
            let auth = serialize_header(&data)?;
            data.ciphertext = low::chacha20poly1305::encrypt(key.data().as_slice(), data.nonce.as_slice(), auth.as_slice(), plaintext.as_slice())?;
            Ok(serialize(&mut data)?)
        }
        _ => {
            return Err(CryptoError::NotImplemented(format!("mode not implemented: {} (try \"chacha20poly1305\")", op.algorithm)));
        }
    }
}

/// Generate a key given a password and a salt
pub fn gen_key(password: &[u8], salt: &[u8], cpu: usize, mem: usize) -> CResult<Key> {
    Ok(Key::new(low::gen_key(password, salt, cpu, mem)?))
}

/// Generate a random hex string (64 bytes). In Turtl js, this was done by
/// hashing a time value concatenated with a UUID, but why bother when we can
/// just generate the "hash" directly by converting 32 random bytes to a hex
/// string?
pub fn random_hash() -> CResult<String> {
    low::to_hex(&low::rand_bytes(32)?)
}

