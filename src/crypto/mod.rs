//! A higher-level turtl-core-specific cryptographic module.
//!
//! We make extensive use of the wrapped crypto primitives in the crypto::low
//! module here to provide the pieces used for cryptography used throughout
//! turtl-core.
//!
//! This includes high-level encryption/decryption as well as (de)serialiation
//! from/to [Turtl's standard format](https://turtl.it/docs/security/encryption-specifics/).

#[macro_use]
mod low;

pub use ::crypto::low::{
    CResult,
    CryptoError,
    PadMode,
    Hasher,
    to_hex,
    from_hex,
    to_base64,
    from_base64,
    sha256,
    sha512,
};

/// Stores our current crypto version. This gets encoded into a header in the
/// ciphertext and lets the crypto module know how to handle the message.
const CRYPTO_VERSION: u16 = 5;

/// Stores which ciphers we have available.
///
/// The index for this is used in the crypto `desc` field (see deserialize() for
/// more info). For this reason, the order of this array must never change: new
/// items must only be appended.
const CIPHER_INDEX: [&'static str; 1] = ["aes"];

/// Stores which block modes we have available.
///
/// The index for this is used in the crypto `desc` field (see deserialize() for
/// more info). For this reason, the order of this array must never change: new
/// items must only be appended.
const BLOCK_INDEX: [&'static str; 2] = ["cbc", "gcm"];

/// Stores which pad modes we have available.
///
/// The index for this is used in the crypto `desc` field (see deserialize() for
/// more info). For this reason, the order of this array must never change: new
/// items must only be appended.
const PAD_INDEX: [&'static str; 2] = ["AnsiX923", "PKCS7"];

/// Stores which pad modes we have available.
///
/// The index for this is used in the crypto `desc` field (see deserialize() for
/// more info). For this reason, the order of this array must never change: new
/// items must only be appended.
const KDF_INDEX: [&'static str; 1] = ["SHA256:2:64"];

/// Find the position of a static string in an array of static strings
fn find_index(arr: &[&'static str], val: &str) -> CResult<usize> {
    for i in 0..arr.len() {
        if val == arr[i] { return Ok(i) }
    }
    Err(CryptoError::Msg(format!("not found: {}", val)))
}

#[derive(Debug)]
/// Describes how we want to run our encryption.
pub struct CryptoOp {
    cipher: &'static str,
    blockmode: &'static str,
    iv: Option<Vec<u8>>,
    utf8_random: Option<u8>,
}

impl CryptoOp {
    /// Create a new crypto op with a cipher/blockmode
    pub fn new(cipher: &'static str, blockmode: &'static str) -> CResult<CryptoOp> {
        try!(find_index(&CIPHER_INDEX, cipher));
        try!(find_index(&BLOCK_INDEX, blockmode));
        Ok(CryptoOp { cipher: cipher, blockmode: blockmode, iv: None, utf8_random: None })
    }

    /// Create a new crypto op with a cipher/blockmode/iv
    #[allow(dead_code)]
    pub fn new_with_iv(cipher: &'static str, blockmode: &'static str, iv: Vec<u8>) -> CResult<CryptoOp> {
        let mut op = try!(CryptoOp::new(cipher, blockmode));
        op.iv = Some(iv);
        Ok(op)
    }

    #[allow(dead_code)]
    /// Create a new crypto op with a cipher/blockmode/iv/utf8 byte
    pub fn new_with_iv_utf8(cipher: &'static str, blockmode: &'static str, iv: Vec<u8>, utf8: u8) -> CResult<CryptoOp> {
        let mut op = try!(CryptoOp::new(cipher, blockmode));
        op.iv = Some(iv);
        op.utf8_random = Some(utf8);
        Ok(op)
    }
}

#[derive(Debug)]
/// Describes some meta about our payload. This includes the cipher used, the
/// block mode used, and possibly which padding and key-derivation methods used.
pub struct PayloadDescription {
    pub cipher_index: u8,
    pub block_index: u8,
    pub pad_index: Option<u8>,
    pub kdf_index: Option<u8>,
}

impl PayloadDescription {
    /// Create a new PayloadDescription from a crypto version and some other data
    pub fn new(crypto_version: u16, cipher: &str, block: &str, padding: Option<&str>, kdf: Option<&str>) -> CResult<PayloadDescription> {
        if crypto_version == 0 { return Err(CryptoError::Msg(format!("PayloadDescription not implemented for version 0"))); }

        let mut desc: Vec<u8> = Vec::with_capacity(4);
        desc.push(try!(find_index(&CIPHER_INDEX, cipher)) as u8);
        desc.push(try!(find_index(&BLOCK_INDEX, block)) as u8);
        if crypto_version <= 4 {
            if let Some(ref x) = padding { desc.push(try!(find_index(&PAD_INDEX, x)) as u8); }
            if let Some(ref x) = kdf { desc.push(try!(find_index(&KDF_INDEX, x)) as u8); }
        }
        Ok(try!(PayloadDescription::from(desc.as_slice())))
    }

    /// Convert a byte vector into a PayloadDescription object
    pub fn from(data: &[u8]) -> CResult<PayloadDescription> {
        if data.len() < 2 { return Err(CryptoError::Msg(format!("PayloadDescription::from - bad desc length (< 2)"))) }
        let cipher = data[0];
        let block = data[1];
        let pad = if data.len() > 2 { Some(data[2]) } else { None };
        let kdf = if data.len() > 3 { Some(data[3]) } else { None };
        Ok(PayloadDescription {
            cipher_index: cipher,
            block_index: block,
            pad_index: pad,
            kdf_index: kdf,
        })
    }

    /// Covert this payload description into a byte vector
    pub fn as_vec(&self) -> Vec<u8> {
        let mut desc: Vec<u8> = Vec::new();
        desc.push(self.cipher_index);
        desc.push(self.block_index);
        match self.pad_index {
            Some(x) => desc.push(x),
            None => return desc,
        }
        match self.kdf_index {
            Some(x) => desc.push(x),
            None => return desc,
        }
        desc
    }

    /// Estimate the size (in bytes) this PayloadDescription is
    pub fn len(&self) -> usize {
        let mut len: usize = 2;
        if self.pad_index.is_some() { len += 1; }
        if self.kdf_index.is_some() { len += 1; }
        len
    }
}

#[derive(Debug)]
/// Holds a deserialized description of our ciphertext.
pub struct CryptoData {
    pub version: u16,
    pub desc: PayloadDescription,
    pub iv: Vec<u8>,
    pub hmac: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl CryptoData {
    /// Create a new CryptoData object from its parts
    pub fn new(version: u16, desc: PayloadDescription, iv: Vec<u8>, hmac: Vec<u8>, ciphertext: Vec<u8>) -> CryptoData {
        CryptoData {
            version: version,
            desc: desc,
            iv: iv,
            hmac: hmac,
            ciphertext: ciphertext,
        }
    }

    /// Returns the estimated serialized size of this CryptoData
    pub fn len(&self) -> usize {
        return (2 + 1 + self.desc.len() + self.iv.len() + self.hmac.len() + self.ciphertext.len()) as usize;
    }
}

/// Deserialize a serialized cryptographic message. Basically, each piece of
/// crypto data in Turtl has a header, followed by N bytes of ciphertext in
/// the following format:
///
///     |-2 bytes-| |-1 byte----| |-N bytes-----------| |-16 bytes-| |-N bytes-------|
///     | version | |desc length| |payload description| |    iv    | |ciphertext data|
///
/// - `version` tells us the serialization version. although it will probably
///   not get over 255, it has two bytes just in case. never say never.
/// - `desc length` is the length of the payload description, which may change
///   in length from version to version.
/// - `payload description` tells us what algorithm/format the encryption uses.
///   This holds 1-byte array indexes for our crypto values (CIPHER_INDEX,
///   BLOCK_INDEX, PAD_INDEX, KDF_INDEX), which tells us the cipher, block mode,
///   etc (and how to decrypt this data).
/// - `iv` is the initial vector of the payload.
/// - `ciphertext data` is our actual encrypted data.
///
/// Note that in older versions (1 <= v <= 4), we used aes-cbc with 
/// encrypt-then-hmac, so our format was as follows:
///
///     |-2 bytes-| |-32 bytes-| |-1 byte----| |-N bytes-----------| |-16 bytes-| |-N bytes-------|
///     | version | |   HMAC   | |desc length| |payload description| |    IV    | |ciphertext data|
///
/// Since we now use authenticated crypto (ie gcm), this format is no longer
/// used, however *all* old serialization versions need to be supported.
///
/// Also note that we basically skipped versions 1 and 2. There is no detectable
/// data that uses those modes, and they can be safely ignored in both their
/// implementation and their tests, although theoretically they use the exact
/// same format as v4.
pub fn deserialize(serialized: &Vec<u8>) -> CResult<CryptoData> {
    let mut idx: usize = 0;
    let mut hmac: Vec<u8> = Vec::new();
    let version: u16 = ((serialized[idx] as u16) << 8) + (serialized[idx + 1] as u16);
    idx += 2;

    // Check if we're at version 0. Version 0 was the very first crypto format.
    //
    // This is a great format because it's essentially unversionable, and the
    // only way we can tell what version it is is by realizing that the lowest
    // two-byte version number base64 can yield is 11,051, so any version
    // number that's ridiculously high is actually version 0. It's also a great
    // format because it was hardcoded to be non-base64 decodable until the
    // :i98ac08bac09a8 junk at the end was removed. Also, it was aes-cbc with no
    // authentication.
    //
    // TODO: if we ever get above 2000 versions (not bloody likely), change
    // this. The lowest allowable Base64 message is '++', which translates to
    // 11,051 but for now we'll play it safe and cap at 2K
    if version > 2000 {
        return deserialize_version_0(serialized);
    }

    if version <= 4 {
        hmac = Vec::from(&serialized[idx..(idx + 32)]);
        idx += 32;
    }

    let desc_length = serialized[idx];
    let desc = &serialized[(idx + 1)..(idx + 1 + (desc_length as usize))];
    let desc_struct = try!(PayloadDescription::from(desc));
    idx += (desc_length as usize) + 1;

    let iv = &serialized[idx..(idx + 16)];
    idx += 16;

    let ciphertext = &serialized[idx..];

    Ok(CryptoData::new(version, desc_struct, Vec::from(iv), hmac, Vec::from(ciphertext)))
}

/// Serialize a CryptoData container into a raw header vector. This is useful
/// for extracting authentication data.
pub fn serialize_header(data: &CryptoData) -> CResult<Vec<u8>> {
    let mut ser: Vec<u8> = Vec::with_capacity(data.len());

    // add the two-byte version
    ser.push((data.version >> 8) as u8);
    ser.push((data.version & 0xFF) as u8);

    // if we're 1 <= version <= 4, append our hmac
    if data.version <= 4 {
        ser.append(&mut data.hmac.clone());
    }

    // add description length and description
    ser.push(data.desc.len() as u8);
    ser.append(&mut data.desc.as_vec().clone());

    // add the iv
    ser.append(&mut data.iv.clone());

    Ok(ser)
}

/// Serialize a CryptoData container into a vector of bytes. For more info on
/// the different serialization formats, check out deserialize().
pub fn serialize(data: &mut CryptoData) -> CResult<Vec<u8>> {
    if data.version == 0 { return serialize_version_0(data); }

    // grab the raw header
    let mut ser = try!(serialize_header(data));

    // lastly, our ciphertext
    ser.append(&mut data.ciphertext);

    Ok(ser)
}

/// Given a master key, derive an encryption key and an authentication key
/// using PBKDF2 as a key-stretcher. Note that HKDF would be a better option but
/// PBKDF2 works just fine. The general consensus is that using it for this
/// purpose is great, but HKDF would be slightly better.
///
/// Also, this is really to support back-wards compatible crypto versions <= 4.
fn derive_keys(master_key: &[u8], desc: &PayloadDescription) -> CResult<(Vec<u8>, Vec<u8>)> {
    match desc.kdf_index {
        Some(x) => {
            let hasher: low::Hasher;
            let iterations: usize;
            let keylen: usize;

            // NOTE: only one KDF value was ever implemented. So instead of
            // doing the "right thing" and parsing it out, I'm going to just
            // hardcode this shit.
            match x {
                0 | _ => {
                    hasher = low::Hasher::SHA256;
                    iterations = 2;
                    keylen = 64;
                }
            }

            let derived = try!(low::pbkdf2(hasher, master_key, &[], iterations, keylen));
            Ok((Vec::from(&derived[0..32]), Vec::from(&derived[32..])))
        },
        None => Err(CryptoError::Msg(format!("derive_keys: no kdf present in desc"))),
    }
}

/// HMACs a CryptoData and compares the generated hash to the hash stored
/// in the struct in constant time. Returns true of the data authenticates
/// properly.
///
/// NOTE that we are not going to implement this correctly. No. Instead, we're
/// going to mimick the way the function worked in the turtl js project, which
/// used string concatenation as such:
///
///   // if version = 4 and desc = "desc"
///   var auth = version + desc.length + desc
///
/// This was (at the time) thought to yield something like "43desc", but since
/// js actually does the combination left to right, it ends up being 7desc
/// (4+3+'desc'). Great. So, let's replicate this then never speak of it again.
/// Luckily this only applies to old versions (new versions use GCM, no need to
/// do the HMAC ourselves).
pub fn authenticate(data: &CryptoData, hmac_key: &[u8]) -> CResult<()> {
    let mut auth: Vec<u8> = Vec::new();
    let shitty_version = data.version + (data.desc.len() as u16);
    let shitty_version_char = match format!("{}", shitty_version).chars().nth(0) {
        Some(x) => x as u8,
        None => 0 as u8,
    };
    auth.push(shitty_version_char);
    auth.append(&mut Vec::from(data.desc.as_vec().as_slice()));
    auth.append(&mut Vec::from(data.iv.as_slice()));
    auth.append(&mut Vec::from(data.ciphertext.as_slice()));
    let hmac = try!(low::hmac(low::Hasher::SHA256, hmac_key, auth.as_slice()));
    if !low::const_compare(hmac.as_slice(), data.hmac.as_slice()) {
        return Err(CryptoError::Authentication(format!("HMAC authentication failed")));
    }
    Ok(())
}

/// Wow. Such fail. In older versions of Turtl, keys were UTF8 encoded strings.
/// This function converts the keys back to raw byte arrays. Ugh.
fn fix_utf8_key(key: &Vec<u8>) -> CResult<Vec<u8>> {
    if key.len() == 32 { return Ok(key.clone()); }

    let keystr = try_c!(String::from_utf8(key.clone()));
    let mut fixed_key: Vec<u8> = Vec::with_capacity(32);
    for char in keystr.chars() {
        fixed_key.push(char as u8);
    }
    Ok(fixed_key)
}

/// Decrypt a message, given a key and the ciphertext. The ciphertext should
/// contain *all* the data needed to decrypt the message encoded in a header
/// (see deserialize() for more info).
pub fn decrypt(key: &Vec<u8>, ciphertext: &Vec<u8>) -> CResult<Vec<u8>> {
    // if our keylen is > 32, it means our key is utf8-encoded. lol. LOOOL.
    // Obviously I should have used utf9. That's the best way to encode raw
    // binary data.
    let key = &try!(fix_utf8_key(&key));

    let deserialized = try!(deserialize(ciphertext));
    let version = deserialized.version;
    let desc = &deserialized.desc;
    let iv = &deserialized.iv;
    let ciphertext = &deserialized.ciphertext;
    let mut decrypted: Vec<u8>;

    if version == 0 {
        decrypted = try!(low::aes_cbc_decrypt(key.as_slice(), iv.as_slice(), ciphertext.as_slice()));
    } else if version <= 4 {
        let (crypt_key, hmac_key) = try!(derive_keys(key.as_slice(), &desc));
        try!(authenticate(&deserialized, hmac_key.as_slice()));
        decrypted = try!(low::aes_cbc_decrypt(crypt_key.as_slice(), iv.as_slice(), ciphertext.as_slice()));
    } else if version >= 5 {
        let auth: Vec<u8> = try!(serialize_header(&deserialized));
        decrypted = try!(low::aes_gcm_decrypt(key.as_slice(), iv.as_slice(), ciphertext.as_slice(), auth.as_slice()));
    } else {
        return Err(CryptoError::NotImplemented(format!("version not implemented: {}", version)));
    }

    // if we're version 4 or 5, we have a stupid UTF8 byte. another vestigial
    // javascript annoyance
    if version >= 4 && version <= 5 {
        decrypted = Vec::from(&decrypted[1..]);
    }

    Ok(decrypted)
}

/// Encrypt a message, given a key and the plaintext. This returns the
/// ciphertext serialized via Turtl serialization format (see deserialize() for
/// more info).
///
/// Note that this function *ONLY* supports encrypting the current crypto
/// version (CRYPTO_VERSION). The idea is that later versions are most likely
/// more secure or correct than earlier versions, so we just don't allow going
/// back in time (although decrypt() support all previous versions).
///
/// That said, some parts of the app *need* version 0 crypto. See encrypt_v0()
/// for this.
pub fn encrypt(key: &Vec<u8>, mut plaintext: Vec<u8>, op: CryptoOp) -> CResult<Vec<u8>> {
    // if our keylen is > 32, it means our key is utf8-encoded. lol. LOOOL.
    // Obviously I should have used utf9. That's the best way to encode raw
    // binary data.
    let key = &try!(fix_utf8_key(&key));
    let version = CRYPTO_VERSION;

    match (op.cipher, op.blockmode) {
        ("aes", "gcm") => {
            let iv = match op.iv {
                Some(x) => x,
                None => try!(random_iv()),
            };

            let mut plaintext_utf;
            // in javascript, we used strings for everything. the problem with
            // this is that plaintext was often utf8-encoded. we needed a way to
            // let the decryptor know that this data should be returned as utf8
            // or as binary. ugh.
            //
            // this is done by prepending a (random) byte to our plaintext such
            // that if the byte is < 128, it's not utf8, if it's >= 128, it's
            // utf8-encoded. since we always use binary in the rust version, we
            // just set this to off (AND 0b1111111)
            if version >= 4 && version <= 5 {
                plaintext_utf = Vec::with_capacity(1 + plaintext.len());
                let utf8_byte = match op.utf8_random {
                    Some(x) => x,
                    None => try!(low::rand_bytes(1))[0] & 0b1111111,
                };
                plaintext_utf.push(utf8_byte);
                plaintext_utf.append(&mut plaintext);
            } else {
                plaintext_utf = plaintext;
            }
            let plaintext = plaintext_utf;

            let desc = try!(PayloadDescription::new(version, op.cipher, op.blockmode, None, None));
            let mut data = CryptoData::new(version, desc, iv, Vec::new(), Vec::new());
            let auth = try!(serialize_header(&data));
            data.ciphertext = try!(low::aes_gcm_encrypt(key.as_slice(), data.iv.as_slice(), plaintext.as_slice(), auth.as_slice()));
            Ok(try!(serialize(&mut data)))
        }
        _ =>  return Err(CryptoError::NotImplemented(format!("mode not implemented: {}-{} (try \"aes-gcm\")", op.cipher, op.blockmode))) ,
    }
}

/// Generate a random cryptographic key (256-bit).
pub fn random_key() -> CResult<Vec<u8>> {
    low::rand_bytes(32)
}

/// Generate a random IV for use with encryption. This is a helper to enforce
/// the idea that we should not reuse IVs.
pub fn random_iv() -> CResult<Vec<u8>> {
    low::rand_bytes(low::aes_block_size())
}

/// Generate a cryptographic key, given a password/salt combination. We also
/// specify the hasher we want to use (Hasher::SHA1, Hasher::SHA256, ...) along
/// with the number of iterations to run our key generator (the more iterations,
/// the harder to crack, so juice that baby up).
///
/// Note that under the hood, this uses PBKDF2. People rag on PBKDF2, but if you
/// use enough iterations, it's fine (and battle-tested).
///
/// Generated keys are always 256-bit.
pub fn gen_key(hasher: Hasher, pass: &str, salt: &[u8], iter: usize) -> CResult<Vec<u8>> {
    low::pbkdf2(hasher, pass.as_bytes(), salt, iter, 32)
}

#[allow(dead_code)]
/// Generate a v4 UUID (random). I'd use the uuid crate, but I don't actually
/// know where it gets its random values from, and I'd rather use our source
/// than depend on it to be truly random when we already have that implemented.
pub fn uuid() -> CResult<String> {
    // generate 15 random bytes, which is exactly what we need for a 36-char
    // UUID (when factoring in the dashes, '4', and the [8,9,a,b] byte). then
    // convert the bytes to hex.
    let rand = try!(low::to_hex(&try!(low::rand_bytes(15))));
    let yvals = ['8', '9', 'a', 'b'];
    let mut uuid = String::new();
    let mut i = 0;
    for char in rand.chars() {
        // match on our counter to insert the correct characters in the right
        // spots. note than whenever we push a character to our UUID, we inc i
        // to keep it in-sync with our string length. this makes things less
        // confusing when matching later on when we know in our heart of hearts
        // that a certain character needs to go in a certain spot.
        match i {
            8 | 23 => {
                uuid.push('-');
                i += 1;
            }
            13 => {
                uuid.push('-');
                i += 1;
                uuid.push('4');
                i += 1;
            },
            18 => {
                uuid.push('-');
                i += 1;
                // grab a random 8 || 9 || a || b character and push it
                let idx = ((yvals.len() as f64) * try!(low::rand_float())).floor() as usize;
                uuid.push(yvals[idx]);
                i += 1;
            },
            _ => {},
        }
        uuid.push(char);
        i += 1;
    }
    Ok(uuid)
}

#[allow(dead_code)]
/// Generate a random hex string (64 bytes). In Turtl js, this was done by
/// hashing a time value concatenated with a UUID, but why bother when we can
/// just generate the "hash" directly by converting 32 random bytes to a hex
/// string?
pub fn random_hash() -> CResult<String> {
    low::to_hex(&try!(low::rand_bytes(32)))
}

#[allow(dead_code)]
/// Version 0 is at the bottom because it's despicable. It should never have
/// existed, yet here we are. Here's the format:
///
///   [n bytes base64 string ciphertext]:i[32 bytes hex iv]
///
/// DDDDDDDDDDDDDDD88$..,,IZ8DDDD8O88OO88888D888888888888888888O88OODD88DOO88O
/// DDDDDDDDDDDDD88O87..,,,D888OOOOOOOOO888OO8DD888888O88888888OO8OZ8DD8DOO88O
/// 8DDDDDD88DDD888O8I..,::~D88888888888O888O888888888O88O88888888ZZ88888OO88O
/// 8DD8D888D888OO888I...,::ZD8888888888888888888888888O888888888OOOZ8888O888O
/// 888DD888888O8OO88? ..,::,88888D8888888888888888888OO8OO88888OO88Z8888OO8OZ
/// 8888D8D8888OO8OO8+ ..,,::~D88DD88DD88888888888888ZOO8OO88888OOOOZ8888OO8OZ
/// D88DD8D8888OOOOOO~...,,:::788DDD888D88888888888O88OO88Z88888OOOOOO888O88OO
/// 88888D8D88DOOOO8O~....,:::,D888888D888888888888OO88O88O888888O8OOZOOOO8OOO
/// D8888DDDDDDOOOOOO~....,::::~D888888888888O88888OOOOOOOOOOO8OZOOOOOOOOOOOZO
/// 88D888DDDDD8O88O8:....,:::::I88D888888888O8888OOOOOOOOO8OO8OOOOOOO8OOOOOZO
/// 888888DDDDDD88OO8:.....,::::,D88888888888888888OOOOOOOOOOOOOZOOOOOOOOOOOZZ
/// DDD88DDDDDDD88OO8:. ...,:::::~D888888888888888888OOOOZOO8O8OZOOOOOOOOOOOZO
/// D8888D8DDDDD8OOO8:. ...,::::::$8888888888888888O8OOOOOOO888OZ8OOOOOOOOOOZO
/// 8DD8DD8DDDD888888,  ...,::::::,888888888888888OO8OOOOOOOOO8OOOOOO8OOOZOO$O
/// D?88D88DDDD888888,. ....,:::::::D88888888888888888O8OOOOO88OO88OOO8OOOOOZO
/// I$DD8DDDDDDD8888D,. ....,:::::::ID8D8888888888888888O8OOO88ZOO88O88OOOOOZZ
/// +DDDDDDDDDDD88D8D,. ....,::::::,,DDD88888888888888888888O88O8O8OO8OOOOOOZO
/// ID8888DDDDDDDDDDD..  ...,,:::::::~DD8D888888888888DDD888888O888OO8OOOOOOZZ
/// D8OOZ8$ZDDD88888D..  ...,,:::::::,78D888888888888DDDD888888O8888OOOOOOOOO$
/// DD8OZ878DDD88888D..   ...,:::::,,:,D8888888888888DDDDDD888OZ888O888OOOZZZZ
/// D88$OO7DD888888O8..   ...,,::::,,::~8888888888888DDDDDD8888O88OO88OOOOZOOO
/// ZO7+??78888888OZO.    ...,,::::::,:,Z8888OO888888DDDDDDD8888OOO8888OOOOOOO
/// ZD+IZZ87O8888O8O8..   ...,,::,::,:::,88888OOO8888DDDDDDDD88O8OO88888OOOOOO
/// 88OOO8O8888888888..   ...,,:,:,:,::::~8OOOOOOO8888DDDDDDD88OZ$ZOO88OOOOOOO
/// 888OOOOO8888888O8..   ....,,:::,::,,::$OOOOOOOO88DDDDDDDDD888O8O8OOOOOOOOO
/// 888OO8O78888O88O8..   .....,:~:,,,:,:,,8OOOOO8O88DDDDDDDDD8D888888888ZOOOO
/// 8888OOZ7OO8OO88O8..:  ~-=+~.:.:,,,:,,,:+8OOOOO888DDDDDDDDDD888888888OOOOOO
/// OD888O=888OO8OOO8..:  .....,+~,:,,,,,,,.ZZOOOOO88DDDDDDDDDDD888D88888OOOZO
/// ZI+~87O$OOOO8OOO8..:  ......,,,,,,,,,,,,.8ZOOOO88DDDDDDDDDDD8888888888OOZZ
/// O7Z87OOO8ZOOOOOOO..=   D U N C E ,,,,,,,,~8OOOO8DDDDDDDDDDDDDDD8D888888OOO
/// 8OO8OOOOOOZOZOOOO...  .....,,,:,,:,,,,,,.ZOOOO888DDDDDDDDDDDDDDDD8888O88OO
/// O8OOOOZOOO+OOOOOO.      ...,,,:,,,,,,,,~OOZ$OO88DD8DDDDDDDDDDDDD8888888888
/// O88OOOZOOI7OOOOOZ... ......,,,,:::,.IN8ZMDNZ8O88D88DDDDDDDDDDDDD8888888888
/// 8OO8$OOOO?OOOOOOO...  .......:=+ONDO$ZI7$N8ZO888DDDDDDDDDDDDNDDDDD88888888
/// 88Z=O$OO?+8OOOOOOI.,=$8MNMMMMMMZZ7II??+?8MOZO88DDDDDDDDDDDNDNDDDD888888888
/// 8ZOO7????IOZOOOOOOO$:,7Z7$I$NM8Z777???++ZZ?888DDDDDDDDDDNNNNNNDDDDD888888D
/// OOZO8O8+OZ7OOOOOOOO8,~+Z?77$8DZ$7$7?I?+++OO888DDDDDDDDDNNNNNNNDDDDDD888888
/// OOOOOOOOOOZ7$IOOO8OO=,~+I7$ZZ$$778DZ$O?+7O8888DDDDDDDDNNNNNNNDNNDDD8888888
/// OOOOOOOOOOOO=8OO88888,7O77$ZZO8NNDMM$??+=NIO$NDDDDDDDNNNNNNNNNNNDDDDDD8888
/// Z8OOOOOOOOO7ZO8888888?:DMMMO$$Z8888OI?I?~M~8Z~~=ONNNDDDNNNNNNNNNNNDDDDD888
/// I8OO8=+Z$??~8O888D7~.:,~~:==~?7$$$77$7I?OI+NZ=+=?~~+MDNNNNNNNNNNNNNDDDDDD8
/// 8O7IIZ7O78?$OO87...=:~~:~=7~,=IIOZZ$$777O?IM7~=?~+==:=MNNNNNNNNNNNNNDDDDDD
/// OOOZZ?8O+$8O$~...=.,::+,=IZ7IODNO7$OZ$78?IO$I==+==$?~::~MNNMNNNNNNNNNDDDDD
/// ?$O8OOOOOO8:.,.~,+.,I,=,.=Z8I~7$88$$7DDNOI87?==~?~=~??=~=DNNNNNNNNNNNDDDDD
/// =888OOOOOZ.,..:,~~..~,I:..,.+7IZD8$$MMOM?+?I:=+=~$:O=~~I~:ONNNNNNNNNNNDDDD
/// 88DZ8OOOO......:~=..,,,,.,.:$DI$$DM8NI+=:==+++I?+Z7:??:~:::MNNNNNNNNNNDDDD
/// 8=ZO?I?7,....:.+:+,.:,~?Z:,...:~??ODO=::~??:D~?=O?~~:7:?+~:7MNNNNNNNNNNDDD
/// ~887O7OI......,+:+,...I~:=:..=:,~.I?~?:+:~~~=~~=??+I~~~:7~?,MNNNNNNNNNNNND
/// Z?I88887.....+:.::....,,..=.+~?.?,~I:~.::=~=+~$~$+=~:+?~::~~IMNNNNNNNNNNND
/// $OOOOO8...,,,=:,,+......,.+~~,=,~~:+:~...:?:Z~+:OI~+::7~+~+:=NNNNNNNNNNNND
/// $OOO88=......:::,?...:,....,:~.,,,:.:~=...=~,~:.7+~?I:~~~~I~+=MNNNNNNNNNNN
/// 8Z=$7O.=....,=++.,...,,~I.::..:~==:~~~7:..=~,~:,N??,+~7~:~:~~,8NNNNNNNNNNN
/// ?O7$+I....:.,,~~.,...,,=7,7?:I:,7~=7=~+,,,$:$.:=N?7.~:~?+=?:~~+MNNNNNNNNNN
/// 7I$O8.....,.,.,~?:.~:==~+==+~Z+=:=:,~,,:,,I$+,~:NI.=+.,~~++==?=DNNMNNNNNNN
/// O$7$I,,.....~.:=+:.=I+I==::~~,.,.,.~+,=::IIMI?8+M?,.,.+?:+:~:~:~MNNNNNNNNN
/// 7+8Z......,.~.=:~+.~?+I+==:+.,,..:,O?~~~~:=~7?$=N+~..::~~I?=+:::7NNNNNNNNN
/// $O8:.....:...,,,=$I~=?+$=7,:.=...:~==:.~..7?$?8+8~~,?7.=~:~=ZDN8MNMNNNNNNN
/// O8D?:.,......+$:=~I?=I?Z++,.,...,+~=~=+,:+.,==7ZN=+,.:=+????8ZNNMNNNNNNNNN
/// 7I$7$ODZ=:,=.:::~~D??7$++~:.,...=+:O:+I.~+,,:~+DN==,,:~+????MMNNNNNNNNNNNN
/// 88888888...,,:+?Z=$?8=~~~:=.,.:,,:~:~:=:,,,~,?+IMI..,:~=?I??MNNNNNNNNMMNMM
/// 888IO88O...,:~+??INM?=:?,,.,...:=~~=~=+~:=:I=?8IM=...:~=+???MNNNNNNNNMNNNM
/// $I?ZD88O...,:=??I$M++:..:......:~:+O:?$:I+:I~?77D~..,,:~+?I?MNNNNNNNMMNNNM
/// 8DID8IO7..,,:=II78NMMD=..:..,...,,:+~~+:=::===+OD+...,,~+?IIMNNNNNNNMMNMMM
/// $+O7?+=I.....,,=ZNM8+7I..........:?.~=:::~=II778$I...,~=+?IIMNNNNNNNNNMMMM
/// D8OOZZZI.....,.,:$MMNNNMMMMMMNIO7=Z+77~7=?+D78Z8..,,:~+????ONNNNNNNNNMMMMM
/// 88OOOI8Z,......,,:=NMNDMNNNMMMNNNMMMMMMMMMMMMM+..,:~=++?IIINNNNNNNNNNNMMMM
/// 8888$+88=,......,,:=NMD8MNMMMMNMNMMMMMMMMMMM7..,:~=++??I77$MNNNNNNNNNNMMMM
/// 88O+I8O88~,.......,:+MMMMMMMMMMMMMMMMMMMMMO..,:=+++??77ZOZMNNNNNNNNNNNNNNM
/// IZID8888DD+:,............,MMNMMMNDDNNMMMD..,~==+?+?I$O88MNNNNNNNNNNNNNNNMM
/// 7D8OZ7:O?Z8D+~:,......,,.,:,=.,~=?=+==~:~=+++??I7$O8DMMMMNNNNNNNNNNNNNNNMM
/// 8DDDNZ8D78NDDNN?:,,,,,.:O=:.?O7Z$I???+++++?I7$Z8NMMMMMMMMNNNNNNNNNNNNNNNMM
/// ?DD7?NODO8D88ODDDO=~:,....,7Z$?M?=+?I??I7$O8NMMMMMMMMMMMNNNNNNNNNNNNNNNNNN
/// 7I7IDD8DOZ$OO888Z8NMI~:,:::~+I8+++?7$Z8DNMMMMMMMMMMMMMMMNNNNNNNNNNNNNNNNNM
/// DIODD+=Z78O8O88O88DNMMZ:::,:::=??788NMMMMMMMNNMMNMMMMNMMNNNNNNNNNNNNNNNNNM
/// 777Z+DOI+8O8OO88DNDNNMO.D8O7ONMMDZMMMMMMMMNN8D8DNNMMMMMMNNNNNNNNNNNNNNNNMM
/// 888888D8ZO888OZ88DNNNNM$,.III++?8NMMMMMMND88O8D8NMMMMMMMNNNNNNNNNNNNNNNNMN
fn deserialize_version_0(serialized: &Vec<u8>) -> CResult<CryptoData> {
    let ciphertext_base64 = String::from_utf8(Vec::from(&serialized[0..(serialized.len() - 34)])).unwrap();
    let ciphertext = low::from_base64(&ciphertext_base64).unwrap();
    let cutoff: usize = serialized.len() - 32;
    let iv_hex = String::from_utf8(Vec::from(&serialized[cutoff..])).unwrap();
    let iv = low::from_hex(&iv_hex).unwrap();
    let desc = PayloadDescription { cipher_index: 0, block_index: 0, pad_index: None, kdf_index: None };
    Ok(CryptoData::new(0, desc, iv, Vec::new(), ciphertext))
}

#[allow(dead_code)]
/// Ugh. Serialize a CryptoData container to version 0. See "dunce" comment
/// above.
fn serialize_version_0(data: &CryptoData) -> CResult<Vec<u8>> {
    let mut ser = to_base64(&data.ciphertext).unwrap();
    ser.push_str(":i");
    ser.push_str(&to_hex(&data.iv).unwrap());
    Ok(Vec::from(ser.as_bytes()))
}

#[allow(dead_code)]
pub fn encrypt_v0(key: &Vec<u8>, iv: &Vec<u8>, plaintext: &String) -> CResult<String> {
    let desc = try!(PayloadDescription::from(&[0, 0]));
    let enc = try!(low::aes_cbc_encrypt(key.as_slice(), iv.as_slice(), &Vec::from(plaintext.as_bytes()), PadMode::ANSIX923));
    let data = CryptoData::new(0, desc, iv.clone(), Vec::new(), enc);
    let serialized = try!(serialize_version_0(&data));
    Ok(try_c!(String::from_utf8(serialized)))
}

#[cfg(test)]
mod tests {
    //! Tests for our high-level Crypto module interface. Note that many of
    //! these tests come from the [common lisp version of turtl-core](https://github.com/turtl/core-cl),
    //! the predecessor to Rust turtl-core.

    use super::*;

    const TEST_ITERATIONS: usize = 32;

    #[test]
    /// Makes sure our cipher/block/padding indexes are correct. New values can be
    /// added to these arrays, but *MUST* be tested here. Note that we also test for
    /// existence of specific values at specific indexes so re-ordering of these
    /// arrays will make our test fail.
    fn indexes_are_correct() {
        assert_eq!(super::CIPHER_INDEX.len(), 1);
        assert_eq!(super::CIPHER_INDEX[0], "aes");
        assert_eq!(super::BLOCK_INDEX.len(), 2);
        assert_eq!(super::BLOCK_INDEX[0], "cbc");
        assert_eq!(super::BLOCK_INDEX[1], "gcm");
        assert_eq!(super::PAD_INDEX.len(), 2);
        assert_eq!(super::PAD_INDEX[0], "AnsiX923");
        assert_eq!(super::PAD_INDEX[1], "PKCS7");
        assert_eq!(super::KDF_INDEX.len(), 1);
        assert_eq!(super::KDF_INDEX[0], "SHA256:2:64");
    }

    #[test]
    fn can_gen_keys() {
        let username = String::from("andrew@thillygooth.com");
        let password = String::from("this is definitely not the password i use for my bank account. no sir.");

        let key = gen_key(Hasher::SHA256, password.as_ref(), username.as_ref(), 69696).unwrap();
        let keystr = to_hex(&key).unwrap();
        // this hex val used for comparison was grabbed from turtl-js using the
        // same username/password/iterations/hasher.
        assert_eq!(keystr, "381dfffbb503b3ed90cd0a30d57e8d2bdba36e6c0bab274ae1c346ef3b1b9778");
    }

    #[test]
    fn can_decrypt_v0() {
        let key = from_base64(&String::from("WPEpNTwrRE144Y7uLuTmJSIYhc1qoo7OMLvZ3oNwaII=")).unwrap();
        let enc = String::from(r#"VFQYqIsxYBVdVxANyHepjXvowy107j+n9t1bQqcSI2E2CGLscFMnuZJW6vxLz75XuBHKn0lhbC5FFL1HDuXa1bjvj9CSQcuOl96DqQzGs6BuBHBtTZDHovuzlaC+J7eanoAydRmCuz5iZgKNLLWgWox9e3HWcwRrbyGwOAq5Cj/7s0cn4lKDE5K9V/+x3EA4LzB6aOekBcJNPKD9LmV7I2yifELN2+OAJC7jcICYZHa6i0KciLBZUxTeYfpM1vZJ4suWLH5ZdTFdT9SUINbi06WGFyJtTOQrqlzIz2LFHctsm/FDuU8r9bwFc4sYbha/Ej80+z3S7Zjfp40Ra5GW71oLyK6NyuZSjbdK/xShybiqzyEhA6hf6ekH4Mfef0SlGYTKTvCx7bNd+pPJa/R+LkT/qGgDDJkyzqejvP7guhk=:ib043a089740f1d5ed086225eb30063ff"#);
        let dec = decrypt(&key, &Vec::from(enc.as_bytes())).unwrap();
        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(dec_str, "{\"sort\":15,\"type\":\"link\",\"tags\":[\"programming\",\"css\",\"design\"],\"url\":\"http://nicolasgallagher.com/css-drop-shadows-without-images/\",\"title\":\"CSS drop-shadows without images\",\"text\":\"Perhaps a bit dated, but seems to work well. Check [the demo](http://nicolasgallagher.com/css-drop-shadows-without-images/demo/).\"}");
    }

    #[test]
    fn can_decrypt_v3() {
        let key = from_base64(&String::from("HsKTwqcAdzAXSsK2Z8OaOy4RI8OqKnoUw5Miw7BbJcOUT8KOdcO0JwsO")).unwrap();
        let enc = from_base64(&String::from(r#"AAOpckDeBymudt1AnCMpNUWE/3gA53BFCXVfl5eRXR6h2gQAAAAAmF5Li7QHzaJda8AwGom/ZGcFhKUjE9VOot2xxxKgQNop6MOkMq6stbbARt8ltbsVQb8I5wSTddcGUJapB6Spd/O+lZ7neYVBNttIm+kb3mekW4AjSBrNBFGpfqsGzOBp3ZVVpkUBJwlCT3/ZJdUXU9KqFlbHq/1uNesiRtXEugTRM4rtKaWoOvPvFye6msGDIxecdjJjI2tSJv4mCvPqnenPw9HzGGDp6U1s9r/FWtdsGoRfxuDPtIKEzuXm4t4CjxMlx/83fOV7xxE4EneMRTOlRUf8MM0eqNkDqAeDK8YNtOmdJLs3XVXRYGvPvh6eR9WcemLbcliz1gqjEmpc+UTWLrL/XDlbDcCQ2RacvJLoEq6i5ogkMa7XTyjKGhrg"#)).unwrap();
        let dec = decrypt(&key, &enc).unwrap();
        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(dec_str, "{\"type\":\"link\",\"url\":\"http://viget.com/extend/level-up-your-shell-game\",\"title\":\"Level Up Your Shell Game\",\"text\":\"Covers movements, aliases, many shortcuts. Good article.\\n\\nTODO: break out the useful bits into individual notes =]\",\"tags\":[\"bash\",\"shell\"],\"sort\":12}");
    }

    #[test]
    fn can_decrypt_v4() {
        let key = from_base64(&String::from("rYMgzeHsjMupzeUvqBpbsDRO1pBk/JWlJp9EHw3yGPs=")).unwrap();
        let enc = from_base64(&String::from(r#"AAQRk+ROrg4uNqRIwlAJrPOlTupLliAXexfnZpBt2nsCAAQAAAAA8PxvFb3rlGm50n75m4q7aLkif54G7BMiK1cqOAgKIziV7cN3Hyq+d2DggAkpSjnfcJXDDi60SGM+y0kjLUWOIuq0QVOFVF+c9OlhL6eQ5NsgYAr2ElUatg7jwufGbbCS93vItWssCJ3M5h2PTtaHTLtxhI0IrThkqeQYkV7bvK5tKOvo60Vc4pZ0LdAKfulIp3DJ0tmC15Nab2QVNDrQ35WB0tXZIBnloLIG0AkrBZYE+ig7cYK24QM52Z2sPSSQB33cKVe7U4OOZuS4rXBc1xwAhWKom9NZSMTYg6Ke69H4ZZTILZkkW4Qkgt+yIIJf"#)).unwrap();
        let dec = decrypt(&key, &enc).unwrap();
        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(dec_str, "{\"type\":\"link\",\"url\":\"http://www.youtube.com/watch?v=m_0e9VGO05g\",\"title\":\"M-Seven - Electronic Flip - YouTube\",\"text\":\"![image](http://i1.ytimg.com/vi/m_0e9VGO05g/maxresdefault.jpg)  \\n\",\"tags\":[\"electronic\",\"calm\"]}");
    }

    #[test]
    fn can_decrypt_v5() {
        let key = from_base64(&String::from("js8BsJMw2jeqdB/NoidMhP1MDwxCF+XUYf3b+r0fTXs=")).unwrap();
        let enc = from_base64(&String::from(r#"AAUCAAFKp4T7iuwnVM6+OY+nM9ZKnxe5sUvA+WvkVfhAFDYUKQRenSi+m1WCGGWZIigqL12fAvE4AA10MGACEjEbHvxGQ45qQcEiVZuqB3EMgijklXjNA+EcvnbvcOFSu3KJ4i1iTbsZH+KORmqoxGsWYXafq/EeejAMV94umfC0Uwl6cuCOav2OcDced5GYHwkd9qSDTR+SJyjgAq33r7ylVQQASa8YUP7jx/FGoT4mzjp0+rNAoyqGYU7gJz4v0ccUfm34ww1eZS1kkWmy33h5Cqi6R7Y0y57ytye2WXjvlHGC2/iglx7sPgemDCmIoIMBVdXDW5sMw2fxmpQph1pyst10+Wbv4DcNtN+fMpseDdmTpbXarGNFqyYul/QXM9WmzUTtjeW3kZxol989l+1WXrr6E5Ctk61NVb98PtRMFuRHYU8kt3cTUE4m0G8PGwK62vkp+2pI6fn3UijOFLYHDpeGbgRiAeBbykHgXrtXfAIpysl/FOl1NzfFz44="#)).unwrap();
        let dec = decrypt(&key, &enc).unwrap();
        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(dec_str, "{\"type\":\"link\",\"url\":\"http://www.baynatives.com/plants/Erysimum-capitatum/\",\"title\":\"Erysimum capitatum Gallery - Bay Natives: The San Francisco Source for Native Plants\",\"text\":\"![image](http://www.baynatives.com/plants/Erysimum-capitatum/03-P4082478__.jpg)  \\n\",\"tags\":[\"backyard\",\"garden\",\"native plants\",\"bay natives\",\"flower\",\"wildflower\"]}");
    }

    #[test]
    fn can_encrypt_v0() {
        let key = from_base64(&String::from("nDuViJIt1KFY0AKZqkyrVJ/qxeWaC8sD8Ynuo2iT850=")).unwrap();
        let iv = from_base64(&String::from("uPTMrLsYTVBTj3c3LATl/g==")).unwrap();
        let plain = String::from(r#"version 0 violates the NAP"#);
        let enc = encrypt_v0(&key, &iv, &plain).unwrap();
        assert_eq!(enc, "IXORwc/2T+Zt/I+QGUSGzQV4a8/gvHpNHiKMZhOJv1U=:ib8f4ccacbb184d50538f77372c04e5fe");
    }

    #[test]
    fn can_encrypt_latest() {
        let key = from_base64(&String::from("2gtrzmvEQkfK9Lq+0eGqLjDrmlKBabp7T212Zdv35T0=")).unwrap();
        let iv = from_base64(&String::from("gFKdvdDznhM+Wo/uzfsPbg==")).unwrap();
        let plain = String::from(r#"{"title":"libertarian quotes","body":"Moreover, the institution of child labor is an honorable one, with a long and glorious history of good works. And the villains of the piece are not the employers, but rather those who prohibit the free market in child labor. These do-gooders are responsible for the untold immiseration of those who are thus forced out of employment. Although the harm done was greater in the past, when great poverty made widespread child labor necessary, there are still people in dire straits today. Present prohibitions of child labor are thus an unconscionable interference with their lives.","tags":["moron"],"mod":1468007942,"created":1468007942.493,"keys":[]}"#);
        let op = CryptoOp::new_with_iv_utf8("aes", "gcm", iv, 42).unwrap();
        let enc = encrypt(&key, Vec::from(plain.as_bytes()), op).unwrap();
        let enc_str = to_base64(&enc).unwrap();
        assert_eq!(enc_str, "AAUCAAGAUp290POeEz5aj+7N+w9uaa6P93K8GQvTwE7C+PXslD/3BgY5vt6zufAH8VgDTr3OwozHCtz/hBsEmHcH8GTWKC3EMyMO5nlJxttmuCJ22IMtEZqYWv8rosS9rX/XTaI1+c8LJga/0V7vwTs2U1Eaa8EiXRsO3eaitblmZnCTxOE0CB+bNu+yF/YZ8kxhVr5fzpipPFTwabWck/i7piglfELrhCuTKE4oYZzDlprv1CXLvRma3i/O9Vw7GsXKdSwP7tKLQG7yTER7jNd5C4jlpjwB8GCeZ/plam9q3Rg95hpRDe4ZMXSSw44Fraxg8C/JgG4wGyHt/bXNGVHbWEpO2wLgwP2cEMvWX75YSPwYoVlvb773uXiPOlFJAhF9q2BNeBux67t1vPmWuQ43k1HBgIdaaaaRdEDxRIRlctlUE7KU6Z2otD5eDjUQEuYjyygiVWf0s+gn0cFBBfpBV+yX1mn35RTcBrdHSQkHHk/r/YxhKuAw6oabQDJZpMNW7ivFTvPbnaVapVTU8BWdop+4yIKqZt2u/N1melKkKPm3noYxp6xYW+aBZ5P48YQH/O+gWha3rFdz7pt7THbtb2wwZDdwioI4u2S/8BAwKEg8gOqGL9C8GKpay59rsufd+6P3RxjNmGPxbzfN2mCzrNMT6iIFCIzDcGbRfEpmMcFtXjd8GSMY2JDGH06hDEe51HPO8X64H5IUZLWzRKlJOgOhkG9nJLRgNwMsZnH1aNnoZsXWc40brIdnY1/fwQ1aS6cibOD7SaQikXGjQEpXKNqWJReaurSVpuFGZTtJY52QUBQlDEqS8IWftjziopjvpsjgG3f7PSyju0hABA7iZVJ/A3dLaYwCJunhZ49UflEA45nwZs/pyVVkc2hYJfYQwBf6n0Wohta7uVMP0SNoDKhO1rsRg14tjnv7xIoKyLtpNEcGaGuGPY/l+JojHt0sVNFn");
    }

    #[test]
    fn can_gen_random_keys() {
        // test a number of hashes
        for _ in 0..TEST_ITERATIONS {
            let key = random_key().unwrap();
            assert_eq!(key.len(), 32);
        }
    }

    #[test]
    fn can_gen_random_ivs() {
        // test a number of hashes
        for _ in 0..TEST_ITERATIONS {
            let key = random_iv().unwrap();
            assert_eq!(key.len(), 16);
        }
    }

    #[test]
    fn can_gen_random_hash() {
        // test a number of hashes
        for _ in 0..TEST_ITERATIONS {
            let hash = random_hash().unwrap();
            assert_eq!(hash.len(), 64);
            for chr in hash.chars() {
                let cint = chr as u32;
                assert!(
                    (cint >= ('0' as u32) && cint <= ('9' as u32)) ||
                    (cint >= ('a' as u32) && cint <= ('f' as u32))
                );
            }
        }
    }

    #[test]
    fn can_gen_uuid() {
        // since we get a lot of variants, let's generate a lot of these and run
        // the test for each one
        for _ in 0..TEST_ITERATIONS {
            let uuidstr = uuid().unwrap();
            assert_eq!(uuidstr.len(), 36);
            let mut i = 0;
            //println!("uuid: {}", uuidstr);
            for chr in uuidstr.chars() {
                //println!("i/c: {}: {}", chr, i);
                match i {
                    8 | 13 | 18 | 23 => assert_eq!(chr, '-'),
                    14 => assert_eq!(chr, '4'),
                    19 => assert!(chr == '8' || chr == '9' || chr == 'a' || chr == 'b'),
                    _ => {
                        let cint = chr as u32;
                        assert!(
                            (cint >= ('0' as u32) && cint <= ('9' as u32)) ||
                            (cint >= ('a' as u32) && cint <= ('f' as u32))
                        );
                    }
                }
                i += 1;
            }
        }
    }
}

