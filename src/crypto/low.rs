//! The Crypto "low" module: creates a Turtl-standard interface around low
//! level crypto library(s) such that we:
//!
//! - Only define/wrap/expose the crypto primitives of those libraries we need
//! - Keep the wrapper API intentionally turtl-core agnostic...we really only
//!   care about general crypto primitives here
//! - Can swap out the underlying crypto libraries and all that needs to change
//!   are the internals of this wrapper (while still exposing the same API to
//!   the rest of turtl-core)
//!
//! For a higher-level set of turtl-core crypto, check out one module up,
//! ::crypto.
//!
//! Also note that a goal of this module is to only wrap *one* underlying crypto
//! lib, but thanks to various issues with PBKDF2 (which MUST be supported for
//! backwards compat), it currently uses both gcrypt and rust-crypto.

use ::serialize::hex::{ToHex, FromHex};
use ::serialize::base64::{self, ToBase64, FromBase64};
use ::gcrypt;
use ::gcrypt::rand::Random;
use ::rust_crypto;
use ::constant_time_eq;

lazy_static! {
    /// Init the gcrypt lib and store our token
    static ref TOKEN: gcrypt::Token = gcrypt::init(|mut x| {
        x.enable_secmem(1024 * 1024 * 4).unwrap();
        x.enable_secure_rndpool();
    });
    /// Store our block size. This is mainly used for padding.
    static ref AES_BLOCK_SIZE: usize = gcrypt::cipher::CIPHER_AES256.block_len();
}
/// Defines our GCM tag size
const GCM_TAG_LENGTH: usize = 16;
/// Defines flags for our ciphers
const GCRYPT_CIPHER_FLAGS: gcrypt::cipher::Flags = gcrypt::cipher::FLAG_SECURE;
/// Defines flags for our hmacs
const GCRYPT_MAC_FLAGS: gcrypt::mac::Flags = gcrypt::mac::FLAG_SECURE;
/// Defines flags for our random number generation
const GCRYPT_RANDOM_STRENGTH: gcrypt::rand::Level = gcrypt::rand::VERY_STRONG_RANDOM;

quick_error! {
    /// Define a type for cryptography errors.
    #[derive(Debug)]
    pub enum CryptoError {
        Msg(str: String) {
            description(str)
            display("crypto: error: {}", str)
        }
        Authentication(str: String) {
            description("authentication error")
            display("crypto: authentication error: {}", str)
        }
        NotImplemented(str: String) {
            description("not implemented")
            display("crypto: not implemented: {}", str)
        }
    }
}

pub type CResult<T> = Result<T, CryptoError>;

/// Convert an error object into a CryptoError (via printing).
macro_rules! tocterr {
    ($e:expr) => (CryptoError::Msg(format!("crypto error: {}", $e)))
}

/// Like try!, but converts errors found into CryptoErrors.
macro_rules! try_c {
    ($e:expr) => (try!($e.map_err(|e| tocterr!(e))))
}

/// Specifies what type of padding we want to use when encrypting data via CBC.
#[derive(Debug)]
pub enum PadMode {
    #[allow(dead_code)]
    PKCS7,
    ANSIX923,
}

/// Specifies the hash algorithms available for hashing or PBKDF2/HMAC
#[derive(Debug)]
pub enum Hasher {
    SHA1,
    SHA256,
    SHA512,
}

/// Generic hash function that uses the Hasher enum to specify the hash function
/// used for hte given data. Note that this function is not a necessary export
/// for this module, so it remains private.
fn hash(hasher: Hasher, data: &[u8]) -> CResult<Vec<u8>> {
    let hashtype = match hasher {
        Hasher::SHA1 => gcrypt::digest::MD_SHA1,
        Hasher::SHA256 => gcrypt::digest::MD_SHA256,
        Hasher::SHA512 => gcrypt::digest::MD_SHA512,
    };
    let mut result: Vec<u8> = vec![0; hashtype.digest_len()];
    gcrypt::digest::hash(*TOKEN, hashtype, data, &mut result[..]);
    Ok(result)
}

/// SHA1 some data and return a u8 vec result.
#[allow(dead_code)]
pub fn sha1(data: &[u8]) -> CResult<Vec<u8>> {
    hash(Hasher::SHA1, data)
}

/// SHA256 some data and return a u8 vec result.
#[allow(dead_code)]
pub fn sha256(data: &[u8]) -> CResult<Vec<u8>> {
    hash(Hasher::SHA256, data)
}

/// SHA512 some data and return a u8 vec result.
#[allow(dead_code)]
pub fn sha512(data: &[u8]) -> CResult<Vec<u8>> {
    hash(Hasher::SHA512, data)
}

/// Convert a u8 vector to a hex string.
pub fn to_hex(data: &Vec<u8>) -> CResult<String> {
    Ok(data[..].to_hex())
}

/// Convert a hex string to a u8 vector.
pub fn from_hex(data: &String) -> CResult<Vec<u8>> {
    Ok(try_c!(data.from_hex()))
}

/// Convert a u8 vector of binary data inot a base64 string.
pub fn to_base64(data: &Vec<u8>) -> CResult<String> {
    Ok(data[..].to_base64(base64::STANDARD))
}

/// Convert a base64 string to a vector of u8 data.
pub fn from_base64(data: &String) -> CResult<Vec<u8>> {
    match data.from_base64() {
        Ok(x) => Ok(x),
        Err(e) => Err(tocterr!(format!("base64: {}", e))),
    }
}

/// Given a key (password/secret) and a set of data, run an HMAC-SHA256 and
/// return the binary result as a u8 vec.
pub fn hmac(hasher: Hasher, key: &[u8], data: &[u8]) -> CResult<Vec<u8>> {
    let hashtype = match hasher {
        Hasher::SHA1 => gcrypt::mac::HMAC_SHA1,
        Hasher::SHA256 => gcrypt::mac::HMAC_SHA256,
        Hasher::SHA512 => gcrypt::mac::HMAC_SHA512,
    };
    let mut result: Vec<u8> = vec![0; hashtype.mac_len()];
    let mut maccer = try_c!(gcrypt::mac::Mac::new(*TOKEN, hashtype, GCRYPT_MAC_FLAGS));
    try_c!(maccer.set_key(key));
    try_c!(maccer.write(data));
    try_c!(maccer.read(&mut result[..]));
    Ok(result)
}

/// Do a constant-time comparison of two byte arrays.
pub fn const_compare(arr1: &[u8], arr2: &[u8]) -> bool {
    constant_time_eq::constant_time_eq(arr1, arr2)
}

/// Generate N number of CS random bytes.
pub fn rand_bytes(len: usize) -> CResult<Vec<u8>> {
    let mut result: Vec<u8> = vec![0; len];
    (&mut result[..]).randomize(*TOKEN, GCRYPT_RANDOM_STRENGTH);
    Ok(result)
}

/// Generate a random u64. Uses rand_bytes() and bit shifting to build a u64.
pub fn rand_int() -> CResult<u64> {
    let bytes = try!(rand_bytes(8));
    let mut val: u64 = 0;
    for &x in &bytes {
        val = val << 8;
        val += x as u64;
    }
    Ok(val)
}

/// Generate a random floating point (f64) between 0.0 and 1.0. Uses rand_int()
/// and divides it by u64::MAX to get the value.
#[allow(dead_code)]
pub fn rand_float() -> CResult<f64> {
    Ok((try!(rand_int()) as f64) / (::std::u64::MAX as f64))
}

/*
/// Generate a key from a password/salt using PBKDF2/SHA256. This uses gcrypt.
///
/// NOTE that gcrypt does NOT allow zero-length salts, which of course v0 auth
/// generation relies on.
pub fn pbkdf2(hasher: Hasher, pass: &[u8], salt: &[u8], iter: usize, keylen: usize) -> CResult<Vec<u8>> {
    let hashtype = match hasher {
        Hasher::SHA1 => gcrypt::digest::MD_SHA1,
        Hasher::SHA256 => gcrypt::digest::MD_SHA256,
        Hasher::SHA512 => gcrypt::digest::MD_SHA512,
    };
    let mut result: Vec<u8> = vec![0; keylen];
    try_c!(gcrypt::kdf::pbkdf2_derive(*TOKEN, hashtype, iter as u32, pass, salt, &mut result[..]));
    Ok(result)
}
*/

/*
/// Generate a key from a password/salt using PBKDF2/SHA256. This uses openssl.
///
/// NOTE that openssl requires the salt be a utf8 string, so cannot possibly
/// work with binary data. useless.
pub fn pbkdf2(hasher: Hasher, pass: &[u8], salt: &[u8], iter: usize, keylen: usize) -> CResult<Vec<u8>> {
    let pbfn = match hasher {
        Hasher::SHA1 => pkcs5::pbkdf2_hmac_sha1,
        Hasher::SHA256 => pkcs5::pbkdf2_hmac_sha256,
        Hasher::SHA512 => pkcs5::pbkdf2_hmac_sha512,
    };

    let mut pass_str = String::new();
    let mut i = 0;
    for byte in pass {
        pass_str.push(*byte as char);
        i += 1;
        println!("byte: {} -- {} / {}", byte, i, pass_str.len());
    }
    println!("- pass: {}", to_base64(&Vec::from(pass_str.as_bytes())).unwrap());
    Ok(pbfn(&pass_str, salt, iter, keylen))
}
*/

/// Generate a key from a password/salt using PBKDF2/SHA256. This uses
/// ruct-crypto.
pub fn pbkdf2(hasher: Hasher, pass: &[u8], salt: &[u8], iter: usize, keylen: usize) -> CResult<Vec<u8>> {
    let mut result: Vec<u8> = vec![0; keylen];
    match hasher {
        Hasher::SHA1 => {
            let mut hmac = rust_crypto::hmac::Hmac::new(rust_crypto::sha1::Sha1::new(), pass);
            rust_crypto::pbkdf2::pbkdf2(&mut hmac, salt, iter as u32, &mut result);
        },
        Hasher::SHA256 => {
            let mut hmac = rust_crypto::hmac::Hmac::new(rust_crypto::sha2::Sha256::new(), pass);
            rust_crypto::pbkdf2::pbkdf2(&mut hmac, salt, iter as u32, &mut result);
        },
        Hasher::SHA512 => {
            let mut hmac = rust_crypto::hmac::Hmac::new(rust_crypto::sha2::Sha512::new(), pass);
            rust_crypto::pbkdf2::pbkdf2(&mut hmac, salt, iter as u32, &mut result);
        },
    }
    Ok(result)
}

/// Pad a byte vector using padding of type PadMode.
///
/// Note that most crypto libs either pad for you or assume you have done your
/// own padding, so we implement ours here. Also note that this is only actually
/// used in a handful of places, where later crypto primitives (AES-GCM, for
/// instance) can't be used due to backwards compat issues.
fn pad(data: &mut Vec<u8>, pad_mode: PadMode) {
    let blocksize: usize = *AES_BLOCK_SIZE;
    let mut pad_len = blocksize - (data.len() % blocksize);
    if pad_len == 0 { pad_len = blocksize; }

    for i in 0..pad_len {
        let val: u8 = match pad_mode {
            // PKCS7:
            //  05 05 05 05 05
            PadMode::PKCS7 => pad_len as u8,
            // ANSIX923:
            //  00 00 00 00 05
            PadMode::ANSIX923 => {
                if i == (pad_len - 1) {
                    pad_len as u8
                } else {
                    0u8
                }
            }
        };
        data.push(val);
    }
}

/// Unpad a byte vector. We do this generically. Both PKCS7 and AnsiX923 store
/// the length of the padded bytes at the end of the data, so all we have to do
/// is grab the last byte and truncate the vector to LEN - LASTBYTE. So easy. A
/// baboon could do it.
fn unpad(data: &mut Vec<u8>) {
    let blocksize: usize = *AES_BLOCK_SIZE;
    let last = match data.last() {
        Some(x) => x + 0,
        None => return
    };

    // if our padding is greater than our block size, the padding is invalid and
    // we proceed without unpadding.
    if (last as usize) > blocksize { return; }

    let datalen = data.len();
    data.truncate(datalen - (last as usize));
}

/// Returns the aes block size. Obviously, always 16, but let's get it straight
/// from the panda's mouth instead of making WILD assumptions.
pub fn aes_block_size() -> usize {
    *AES_BLOCK_SIZE
}

/// Encrypt data using a 256-bit length key via AES-CBC
pub fn aes_cbc_encrypt(key: &[u8], iv: &[u8], data: &[u8], pad_mode: PadMode) -> CResult<Vec<u8>> {
    let mut data = Vec::from(data);
    pad(&mut data, pad_mode);
    let mut enc: Vec<u8> = vec![0; data.len()];
    let mut cipher = try_c!(gcrypt::cipher::Cipher::new(*TOKEN, gcrypt::cipher::CIPHER_AES256, gcrypt::cipher::MODE_CBC, GCRYPT_CIPHER_FLAGS));
    try_c!(cipher.set_key(key));
    try_c!(cipher.set_iv(iv));
    try_c!(cipher.encrypt(&data[..], &mut enc[..]));
    Ok(enc)
}

/// Decrypt data using a 256-bit length key via AES-CBC
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> CResult<Vec<u8>> {
    let mut dec: Vec<u8> = vec![0; data.len()];
    let mut cipher = try_c!(gcrypt::cipher::Cipher::new(*TOKEN, gcrypt::cipher::CIPHER_AES256, gcrypt::cipher::MODE_CBC, GCRYPT_CIPHER_FLAGS));
    try_c!(cipher.set_key(key));
    try_c!(cipher.set_iv(iv));
    try_c!(cipher.decrypt(&data[..], &mut dec[..]));
    unpad(&mut dec);
    Ok(dec)
}

/// Encrypt data using a 256-bit length key via AES-GCM
pub fn aes_gcm_encrypt(key: &[u8], iv: &[u8], data: &[u8], auth: &[u8]) -> CResult<Vec<u8>> {
    let mut tag: Vec<u8> = vec![0; GCM_TAG_LENGTH];
    let mut enc: Vec<u8> = vec![0; data.len()];
    let mut cipher = try_c!(gcrypt::cipher::Cipher::new(*TOKEN, gcrypt::cipher::CIPHER_AES256, gcrypt::cipher::MODE_GCM, GCRYPT_CIPHER_FLAGS));
    try_c!(cipher.set_key(key));
    try_c!(cipher.set_iv(iv));
    try_c!(cipher.authenticate(auth));
    try_c!(cipher.encrypt(data, &mut enc[..]));
    try_c!(cipher.get_tag(&mut tag[..]));
    enc.append(&mut tag);
    Ok(enc)
}

/// Decrypt data using a 256-bit length key via AES-GCM
pub fn aes_gcm_decrypt(key: &[u8], iv: &[u8], data: &[u8], auth: &[u8]) -> CResult<Vec<u8>> {
    let tag_cutoff: usize = data.len() - GCM_TAG_LENGTH;
    let tag = &data[tag_cutoff..];
    let data = &data[0..tag_cutoff];
    let mut dec: Vec<u8> = vec![0; data.len()];
    let mut cipher = try_c!(gcrypt::cipher::Cipher::new(*TOKEN, gcrypt::cipher::CIPHER_AES256, gcrypt::cipher::MODE_GCM, GCRYPT_CIPHER_FLAGS));
    try_c!(cipher.set_key(key));
    try_c!(cipher.set_iv(iv));
    try_c!(cipher.authenticate(auth));
    try_c!(cipher.decrypt(data, &mut dec[..]));
    match cipher.check_tag(&tag[..]) {
        Ok(..) => {},
        Err(e) => return Err(CryptoError::Authentication(format!("{}", e))),
    }
    Ok(dec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;
    use std::vec::Vec;
    use std::collections::HashMap;
    use std::u64;
    //use time::PreciseTime;

    /// Grab a random libertarian string, used for testing our heroic crypto
    /// primitives.
    ///
    /// For those who are offended by the data used, and wish to become even
    /// more offended, check out the [elsbot comment history](https://www.reddit.com/user/elsbot),
    /// an aggregation of comments and statements from American propertarians
    /// that are just too ridiculously good not to read.
    fn get_string(key: &str) -> String {
        let mut quotes: HashMap<&str, String> = HashMap::new();

        quotes.insert("the poor", String::from(r#"I've got no issues helping the poor but that isn't where an income tax is going to go and if anything it would make the poor poorer."#));
        quotes.insert("sweatshops", String::from(r#"They wouldn't be forced, that would be their choice. Thankfully, we are out of our industrial revolution, so no one has to make make that choice anymore. However, in third world countries sweatshops are a very good choice, and I would never want to take that choice away from them."#));
        quotes.insert("global warming benefits", String::from(r#"Is the current trend of what appears to be global warming harmful or beneficial? In the news you hear only the negative side of it. But what about the benefits?"#));
        quotes.insert("informed opinion", String::from(r#"My opinion is that I don't know. I don't care enough even to look at the data. I have seen some things that suggest it exists because of the sun and other that it exists because of CO2. I know that shit has been changing for as long as the earth has been. What I don't like is when people say that most or all scientists agree. This is an appeal to authority and should be disregarded. One thing that makes me skeptical of the claims is that the solution is always more state power in some form. I was all ears, then I was told the governments would need more tax money to forgive my sin fix the problem. Riiiiight. I see whats going on here."#));
        quotes.insert("zoning is communism", String::from(r#"This is a violation of private property rights. They're calling it an issue of zoning. Zoning is communism."#));
        quotes.insert("kill your workforce", String::from(r#"Fire them, if they resist, stike or occupy your premises, buy an M134 Minigun to revolve the dispute, then hire new workers"#));
        quotes.insert("age of consent", String::from(r#"Age of consent laws were basically created and supported by the Christian Woman's Association and feminism in order to decrease the supply of available sex to men in order to make it easier for older ladies to find a mate."#));
        quotes.insert("free baby market", String::from(r#"...in a libertarian society the existence of a free baby market will bring such 'neglect' down to a minimum."#));
        quotes.insert("child labor", String::from(r#"If the options available to a person are work or starve, why would you take away the work option? If people are voluntarily choosing to work in a factory under terrible conditions, it means the alternatives available to them are even worse. That work is an opportunity for them to better themselves. Child labour regulations only hampered the development and expansion of the industries that were providing these opportunities."#));
        quotes.insert("social contract", String::from(r#"I can't seem to find this mythical document called the "social contract" you claim I am party to."#));
        quotes.insert("starve the child", String::from(r#"...the parent should have the legal right not to feed the child, i.e., to allow it to die."#));
        quotes.insert("statistical nonsense", String::from(r#"The problem with your argument is that you are relying on statistics to support your worldview."#));
        quotes.insert("genghis khan", String::from(r#"Genghis Khan has done more good for society and the world than any Democracy that has ever existed..."#));
        quotes.insert("no tolerance", String::from(r#"In a covenant...among proprietor and community tenants for the purpose of protecting their private property, no such thing as a right to free (unlimited) speech exists, not even to unlimited speech on one's own tenant-property. One may say innumerable things and promote almost any idea under the sun, but naturally no one is permitted to advocate ideas contrary to the very covenant of preserving and protecting private property, such as democracy and communism. There can be no tolerance toward democrats and communists in a libertarian social order. They will have to be physically separated and removed from society."#));
        quotes.insert("right to starve", String::from(r#"Also I don't see why right to food is a thing. It's not a thing naturally. You have the right to get yourself food in nature, but you aren't entitled to it. It's survival of the fittest."#));
        quotes.insert("slavery", String::from(r#"Should a business owner be forced to provide goods and services to a citizen against their will? That is the basic question here. If a government or any entity is forcing you against your will to use your knowledge, resources, energy, and money, to do something you do not want to do. We have a word for that, it's slavery."#));
        quotes.insert("security services", String::from(r#"If the market for security weren't warped by the violent imposition of a feckless monopoly ("police"), then maybe this child wouldn't have had to provide such a crude implementation of security services for his family."#));
        quotes.insert("filthy statists", String::from(r#"And I can go on for hours on hundreds of free market solutions that could solve that problem, The statist can only come up with one solution, Make A LAW!!!"#));
        quotes.insert("out of context", String::from(r#"None of which is to say that the Empire isn't sometimes brutal. In Episode IV, Imperial stormtroopers kill Luke's aunt and uncle and Grand Moff Tarkin orders the destruction of an entire planet, Alderaan. But viewed in context, these acts are less brutal than they initially appear. Poor Aunt Beru and Uncle Owen reach a grisly end, but only after they aid the rebellion by hiding Luke and harboring two fugitive droids. They aren't given due process, but they are traitors."#));
        quotes.insert("right to harass", String::from(r#"You would have made the choice yourself. No one is holding a gun to your head and telling you to quit, either. If a boss wants to sexually harass you, that's their right. If you want to quit, that is your right. The market will solve any problems."#));
        quotes.insert("honorable labor", String::from(r#"Moreover, the institution of child labor is an honorable one, with a long and glorious history of good works. And the villains of the piece are not the employers, but rather those who prohibit the free market in child labor. These do-gooders are responsible for the untold immiseration of those who are thus forced out of employment. Although the harm done was greater in the past, when great poverty made widespread child labor necessary, there are still people in dire straits today. Present prohibitions of child labor are thus an unconscionable interference with their lives."#));
        // not elsbot, but i had to throw it in
        quotes.insert("minimum wage", String::from(r#"Is there a 'progressive' response to the idea of the minimum wage being immoral because it violates the non-aggression principle?"#));

        if key == "" {
            let mut keys = quotes.keys();
            let rand = match rand_float() {
                Ok(x) => x,
                Err(..) => 0f64,
            };
            let idx = ((keys.len() as f64) * rand).floor() as usize;
            let randkey = keys.nth(idx).unwrap().clone();
            quotes.get(randkey).unwrap().clone()
        } else if quotes.contains_key(key) {
            quotes.get(key).unwrap().clone()
        } else {
            String::from("<not found>")
        }
    }

    #[test]
    fn can_convert_bytes_to_hex() {
        let res = to_hex(&vec![176u8, 11u8, 85u8]).unwrap();
        assert_eq!(res, "b00b55");
    }

    #[test]
    fn can_convert_hext_to_bytes() {
        let res = from_hex(&String::from("b00b55")).unwrap();
        assert_eq!(res, vec![176u8, 11u8, 85u8]);
    }

    #[test]
    fn can_convert_bytes_to_base64() {
        let res = to_base64(&Vec::from(get_string("the poor").as_bytes())).unwrap();
        assert_eq!(res, "SSd2ZSBnb3Qgbm8gaXNzdWVzIGhlbHBpbmcgdGhlIHBvb3IgYnV0IHRoYXQgaXNuJ3Qgd2hlcmUgYW4gaW5jb21lIHRheCBpcyBnb2luZyB0byBnbyBhbmQgaWYgYW55dGhpbmcgaXQgd291bGQgbWFrZSB0aGUgcG9vciBwb29yZXIu");
    }

    #[test]
    fn can_convert_base64_to_bytes() {
        let res = from_base64(&String::from("VGhleSB3b3VsZG4ndCBiZSBmb3JjZWQsIHRoYXQgd291bGQgYmUgdGhlaXIgY2hvaWNlLiBUaGFua2Z1bGx5LCB3ZSBhcmUgb3V0IG9mIG91ciBpbmR1c3RyaWFsIHJldm9sdXRpb24sIHNvIG5vIG9uZSBoYXMgdG8gbWFrZSBtYWtlIHRoYXQgY2hvaWNlIGFueW1vcmUuIEhvd2V2ZXIsIGluIHRoaXJkIHdvcmxkIGNvdW50cmllcyBzd2VhdHNob3BzIGFyZSBhIHZlcnkgZ29vZCBjaG9pY2UsIGFuZCBJIHdvdWxkIG5ldmVyIHdhbnQgdG8gdGFrZSB0aGF0IGNob2ljZSBhd2F5IGZyb20gdGhlbS4=")).unwrap();
        assert_eq!(String::from_utf8(res).unwrap(), get_string("sweatshops"));
    }

    #[test]
    fn can_sha1() {
        let data = get_string("slavery");
        let hash = to_hex(&sha1(data.as_bytes()).unwrap()).unwrap();
        assert_eq!(hash, "1a32cd2b47f4c60d774d397b9005382bbed9252e");
    }

    #[test]
    fn can_sha256() {
        let data = get_string("global warming benefits");
        let hash = to_hex(&sha256(data.as_bytes()).unwrap()).unwrap();
        assert_eq!(hash, "bb2747436ce21a01d44636f4566e65a60c982dac2f493d2e517089f2d3b03e71");
    }

    #[test]
    fn can_sha512() {
        let data = get_string("informed opinion");
        let hash = to_hex(&sha512(data.as_bytes()).unwrap()).unwrap();
        assert_eq!(hash, "c077cf5be30704b119a0cd4b28947f12b02152543030f649f45dd518636831f71d889d7236eb6041dc4f661c8bc823425269a5f798287badb41fb9ecf750490e");
    }

    #[test]
    fn can_hmac_256() {
        let data = get_string("zoning is communism");
        let keystr = get_string("kill your workforce");
        let key = keystr.as_bytes();
        let res = to_hex(&hmac(Hasher::SHA256, &key, &data.as_bytes()).unwrap()).unwrap();
        assert_eq!(res, "b1a698ee4ea7105e79723dfbab65912dffa01c822038b24fbf413a587f241f10");
    }

    // NOTE: Disabled because the test fails randomly. Luckily we only need this
    // when using old versions of the crypto format (CBC+HMAC) so for now we can
    // cross our fingers and hope it works.
    /*
    #[test]
    fn constant_time_compare() {
        let key1 = from_hex(&String::from("f509a6e0249b014d5a626d819073983cf00e873d1f7cc632ef4687ee839174c1")).unwrap();
        let key2 = from_hex(&String::from("f509a6e0249b014d5a626d819073983cf00e873d1f7cc632ef4687ee839174c1")).unwrap();
        let key3 = from_hex(&String::from("e487cbea0d56adc3cd12e89bb17d6a5ef36effde4b778fe07cd70e426c6d714c")).unwrap();

        let iters = 999999;
        let start1 = PreciseTime::now();
        for _ in 0..iters {
            const_compare(key1.as_slice(), key2.as_slice());
        }
        let end1 = start1.to(PreciseTime::now());
        let start2 = PreciseTime::now();
        for _ in 0..iters {
            const_compare(key1.as_slice(), key3.as_slice());
        }
        let end2 = start2.to(PreciseTime::now());
        

        let run1 = end1.num_nanoseconds().unwrap();
        let run2 = end2.num_nanoseconds().unwrap();
        //assert!((run1 - run2).abs() < 40000000);
    }
    */

    #[test]
    fn random_bytes_works() {
        let bytes = rand_bytes(4).unwrap();
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn random_int_works() {
        let int = rand_int().unwrap();
        assert!(int <= u64::MAX);
    }

    #[test]
    fn random_float_works() {
        let val = rand_float().unwrap();
        assert!(0f64 <= val);
        assert!(val <= 1f64);
    }

    #[test]
    fn can_pbkdf2_sha1() {
        let password = get_string("statistical nonsense");
        let salt = b"czar@turtl.it";
        let iter: usize = 40000;
        let keylen: usize = 32;
        let res = pbkdf2(Hasher::SHA1, password.as_bytes(), salt, iter, keylen).unwrap();
        assert_eq!(to_hex(&res).unwrap(), "679bc18cc5325b54feac36252f8bb91ff47ae7c2a0e512bb09eaed1ac9ff12c7");
    }

    #[test]
    fn can_pbkdf2_sha256() {
        let password = get_string("starve the child");
        let salt = b"czar@turtl.it";
        let iter: usize = 80669;
        let keylen: usize = 32;
        let res = pbkdf2(Hasher::SHA256, password.as_bytes(), salt, iter, keylen).unwrap();
        assert_eq!(to_hex(&res).unwrap(), "c340a8109fa6421844e32b119926fe6d064553609aa30c4939f83da4fe633c5a");
    }

    #[test]
    fn can_aes_cbc_256_encrypt() {
        let plain = get_string("age of consent");
        let key = from_hex(&String::from("e487cbea0d56adc3cd12e89bb17d6a5ef36effde4b778fe07cd70e426c6d714c")).unwrap();
        let iv = from_hex(&String::from("c623f0e62bf7e422799637ff03205184")).unwrap();
        let enc = aes_cbc_encrypt(&key[..], &iv[..], plain.as_bytes(), PadMode::PKCS7).unwrap();
        let encbase = to_base64(&enc).unwrap();
        assert_eq!(encbase, "WchtFlfvntw19wvB5Fkx8Cs0q0AedG+GOOR3VcwiQJ16hReOX7b6dCZw6XfOnuZbxwyrlVUFdE+6btiZ/vJ3SWz0iFOpwjxSTagCSFKx95+r7MGCiy5nW0c/2jbMlva7OVxZd05zW2f4LKzvWcG7t8IvBUxQwpWCDqy+65Xu6w9QDHrCUpCmxE1KX6QCO9AZsuFnoB0V2kdIRlfa2LYdmqsxLyZLeWVvtqgYC7UhmxU0U9dx7hYj8yb4dJzpuoeIdyfUOJzI92CTIF/XwWX+o4h/vO629wJJbxSSLax9110=");
    }

    #[test]
    fn can_aes_cbc_256_decrypt() {
        let encbase = String::from("WchtFlfvntw19wvB5Fkx8Cs0q0AedG+GOOR3VcwiQJ16hReOX7b6dCZw6XfOnuZbxwyrlVUFdE+6btiZ/vJ3SWz0iFOpwjxSTagCSFKx95+r7MGCiy5nW0c/2jbMlva7OVxZd05zW2f4LKzvWcG7t8IvBUxQwpWCDqy+65Xu6w9QDHrCUpCmxE1KX6QCO9AZsuFnoB0V2kdIRlfa2LYdmqsxLyZLeWVvtqgYC7UhmxU0U9dx7hYj8yb4dJzpuoeIdyfUOJzI92CTIF/XwWX+o4h/vO629wJJbxSSLax9110=");
        let enc = from_base64(&encbase).unwrap();
        let key = from_hex(&String::from("e487cbea0d56adc3cd12e89bb17d6a5ef36effde4b778fe07cd70e426c6d714c")).unwrap();
        let iv = from_hex(&String::from("c623f0e62bf7e422799637ff03205184")).unwrap();
        let dec = aes_cbc_decrypt(&key[..], &iv[..], &enc).unwrap();
        let decstr = String::from_utf8(dec).unwrap();
        assert_eq!(decstr, get_string("age of consent"));
    }

    #[test]
    fn can_aes_cbc_256_encrypt_ansix923() {
        let plain = get_string("age of consent");
        let key = from_hex(&String::from("265c4f65060c0fcbbce562ba81664de28f6be5c083c42f42cab0c73b6f48ed30")).unwrap();
        let iv = from_hex(&String::from("0d4b1deb697be38e688e38b3fde63b52")).unwrap();
        let enc = aes_cbc_encrypt(&key[..], &iv[..], plain.as_bytes(), PadMode::ANSIX923).unwrap();
        let encbase = to_base64(&enc).unwrap();
        assert_eq!(encbase, "it5TMi/ySbjQWyCnhVJi+EYGsuoBbGWJuMLfiBHbaZfA7b6y+ygfR+gLLDhC+WdxFmK7KOhqCWxCu7J6c5XgDcyiM8sJ7I+Li18dlj8k+0FwBXrrKsBIw1aE+bGW0tu32zBDmPYOfiG0W3USM5kHTNeggcNURIGwYu2SICIMelLK7FMNN3BvFm3beLMdrxjen2PcmJA8aip/W1BdRzzneDd09TLMLRr0psMUbbad/sKyq4plo3ptYkeVqxkLkZ6DCA2FtfcSKJ1gLAx7YSwhf6gClj1L31cJMD3JbV+uqlM=");
    }

    #[test]
    fn can_aes_cbc_256_decrypt_ansix923() {
        let encbase = String::from("WchtFlfvntw19wvB5Fkx8Cs0q0AedG+GOOR3VcwiQJ16hReOX7b6dCZw6XfOnuZbxwyrlVUFdE+6btiZ/vJ3SWz0iFOpwjxSTagCSFKx95+r7MGCiy5nW0c/2jbMlva7OVxZd05zW2f4LKzvWcG7t8IvBUxQwpWCDqy+65Xu6w9QDHrCUpCmxE1KX6QCO9AZsuFnoB0V2kdIRlfa2LYdmqsxLyZLeWVvtqgYC7UhmxU0U9dx7hYj8yb4dJzpuoeIdyfUOJzI92CTIF/XwWX+o3zegyq4zdw7CCoyN2lCy0E=");
        let enc = from_base64(&encbase).unwrap();
        let key = from_hex(&String::from("e487cbea0d56adc3cd12e89bb17d6a5ef36effde4b778fe07cd70e426c6d714c")).unwrap();
        let iv = from_hex(&String::from("c623f0e62bf7e422799637ff03205184")).unwrap();
        let dec = aes_cbc_decrypt(&key[..], &iv[..], &enc).unwrap();
        let decstr = String::from_utf8(dec).unwrap();
        assert_eq!(decstr, get_string("age of consent"));
    }

    #[test]
    fn can_aes_cbc_256_encrypt_decrypt() {
        let password = get_string("");
        let plaintext = get_string("");
        // make sure your compost pile gets plenty of air
        let salt = b"oh, sandra.";
        let key = pbkdf2(Hasher::SHA256, password.as_bytes(), salt, 69000, 32).unwrap();
        let iv = rand_bytes(16).unwrap();

        let enc = aes_cbc_encrypt(key.as_slice(), iv.as_slice(), plaintext.as_bytes(), PadMode::PKCS7).unwrap();
        let dec = aes_cbc_decrypt(key.as_slice(), iv.as_slice(), enc.as_slice()).unwrap();

        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(plaintext, dec_str);
    }

    #[test]
    fn can_aes_gcm_256_encrypt() {
        let plain = get_string("right to starve");
        let key = from_hex(&String::from("f509a6e0249b014d5a626d819073983cf00e873d1f7cc632ef4687ee839174c1")).unwrap();
        let iv = from_hex(&String::from("8649b4a149cfa0c4ddf0a6054b8511a2")).unwrap();
        let auth = from_hex(&String::from("667265652062616279206d61726b6574")).unwrap();
        let enc = aes_gcm_encrypt(&key[..], &iv[..], plain.as_bytes(), &auth[..]).unwrap();
        let encbase = to_base64(&enc).unwrap();
        assert_eq!(encbase, "ATO8XenPJip+FVuJWnLj7BKEzKtdqQ2zANmHjevyCW4xRFyWps5LRUz16llX9zighTUGBgv4ss53/wR9CbggxoVVMCj9C4l6Hvu++SuXlxW/MtaIXSEtpx3HsUYAyB5GmKhX1I7DcSVdmxL25IaRaw5FfibWPOaIdzNFo3Sf76cQMxxYX+OqIyUD4eUcHjdFqc9N7k9xRw8JOY/wsCC5nuHNX82+prwCTL2Ck34sr1RQdMjHV2yZkgrmaTK/I30Fg75INalfXgzYgA==");
    }

    #[test]
    fn can_aes_gcm_256_decrypt() {
        let encbase = String::from("ATO8XenPJip+FVuJWnLj7BKEzKtdqQ2zANmHjevyCW4xRFyWps5LRUz16llX9zighTUGBgv4ss53/wR9CbggxoVVMCj9C4l6Hvu++SuXlxW/MtaIXSEtpx3HsUYAyB5GmKhX1I7DcSVdmxL25IaRaw5FfibWPOaIdzNFo3Sf76cQMxxYX+OqIyUD4eUcHjdFqc9N7k9xRw8JOY/wsCC5nuHNX82+prwCTL2Ck34sr1RQdMjHV2yZkgrmaTK/I30Fg75INalfXgzYgA==");
        let enc = from_base64(&encbase).unwrap();
        let key = from_hex(&String::from("f509a6e0249b014d5a626d819073983cf00e873d1f7cc632ef4687ee839174c1")).unwrap();
        let iv = from_hex(&String::from("8649b4a149cfa0c4ddf0a6054b8511a2")).unwrap();
        let auth = from_hex(&String::from("667265652062616279206d61726b6574")).unwrap();
        let dec = aes_gcm_decrypt(&key[..], &iv[..], &enc, &auth[..]).unwrap();
        let decstr = String::from_utf8(dec).unwrap();
        assert_eq!(decstr, get_string("right to starve"));
    }

    #[test]
    fn can_aes_gcm_256_encrypt_decrypt() {
        let password = get_string("");
        let plaintext = get_string("");
        // make sure your compost pile gets plenty of air
        let salt = b"oh, sandra.";
        let key = pbkdf2(Hasher::SHA256, password.as_bytes(), salt, 69002, 32).unwrap();
        let iv = rand_bytes(16).unwrap();
        // hardcode what very well might be a header for a turtl message
        let mut auth: Vec<u8> = vec![0, 5, 4, 0, 1, 0, 2];
        auth.append(&mut iv.clone());

        let enc = aes_gcm_encrypt(key.as_slice(), iv.as_slice(), plaintext.as_bytes(), auth.as_slice()).unwrap();
        let dec = aes_gcm_decrypt(key.as_slice(), iv.as_slice(), enc.as_slice(), auth.as_slice()).unwrap();

        let dec_str = String::from_utf8(dec).unwrap();
        assert_eq!(plaintext, dec_str);
    }

    #[test]
    fn gcm_auth_failure() {
        let password = get_string("");
        let plaintext = get_string("");
        // make sure your compost pile gets plenty of air
        let salt = b"oh, sandra.";
        let key = pbkdf2(Hasher::SHA256, password.as_bytes(), salt, 69002, 32).unwrap();
        let iv = rand_bytes(16).unwrap();
        // hardcode what very well might be a header for a turtl message
        let mut auth: Vec<u8> = vec![0, 5, 4, 0, 1, 0, 2];
        auth.append(&mut iv.clone());

        let enc = aes_gcm_encrypt(key.as_slice(), iv.as_slice(), plaintext.as_bytes(), auth.as_slice()).unwrap();
        // let's downgrade the version LOL!
        auth[1] = 4;
        match aes_gcm_decrypt(key.as_slice(), iv.as_slice(), enc.as_slice(), auth.as_slice()) {
            Ok(..) => panic!("Authentication succeeded on bad data!"),
            Err(e) => match e {
                CryptoError::Authentication(..) => {},
                _ => panic!("Non-authentication error: {}", e),
            }
        };
    }
}

