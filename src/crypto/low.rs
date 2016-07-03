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

use std::fmt::Write;
use std::u64;

use ::serialize::hex::{ToHex, FromHex};
use ::serialize::base64::{self, ToBase64, FromBase64};
use ::openssl::crypto::hash::{self, Type};
use ::openssl::crypto::hmac::hmac;
use ::openssl::crypto::rand;
use ::openssl::crypto::pkcs5;
use ::openssl::crypto::symm;

quick_error! {
    #[derive(Debug)]
    /// Define a type for cryptography errors.
    pub enum CryptoError {
        Msg(str: String) {
            description(str)
            display("crypto: error: {}", str)
        }
        /*
        Authentication {
            description("authentication error")
            display("crypto: autentication error")
        }
        */
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

/// Specifies what type of padding we want to use when encrypting data.
pub enum PadMode {
    PKCS7,
    ANSIX923,
}

#[allow(dead_code)]
/// SHA256 some data and return a u8 vec result.
pub fn sha256(data: &[u8]) -> CResult<Vec<u8>> {
    Ok(hash::hash(Type::SHA256, data))
}

#[allow(dead_code)]
/// SHA512 some data and return a u8 vec result.
pub fn sha512(data: &[u8]) -> CResult<Vec<u8>> {
    Ok(hash::hash(Type::SHA512, data))
}

#[allow(dead_code)]
/// Convert a u8 vector to a hex string.
pub fn to_hex(data: &Vec<u8>) -> CResult<String> {
    Ok(data[..].to_hex())
}

#[allow(dead_code)]
/// Convert a hex string to a u8 vector.
pub fn from_hex(data: &String) -> CResult<Vec<u8>> {
    Ok(try_c!(data.from_hex()))
}

#[allow(dead_code)]
/// Convert a u8 vector of binary data inot a base64 string.
pub fn to_base64(data: &Vec<u8>) -> CResult<String> {
    Ok(data[..].to_base64(base64::STANDARD))
}

#[allow(dead_code)]
/// Convert a base64 string to a vector of u8 data.
pub fn from_base64(data: &String) -> CResult<Vec<u8>> {
    Ok(try_c!(data.from_base64()))
}

#[allow(dead_code)]
/// Given a key (password/secret) and a set of data, run an HMAC-SHA256 and
/// return the binary result as a u8 vec.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> CResult<Vec<u8>> {
    Ok(hmac(Type::SHA256, key, data))
}

#[allow(dead_code)]
/// Generate N number of CS random bytes.
pub fn rand_bytes(len: usize) -> CResult<Vec<u8>> {
    Ok(rand::rand_bytes(len))
}

#[allow(dead_code)]
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

#[allow(dead_code)]
/// Generate a random floating point (f64) between 0.0 and 1.0. Uses rand_int()
/// and divides it by u64::MAX to get the value.
pub fn rand_float() -> CResult<f64> {
    Ok((try!(rand_int()) as f64) / (u64::MAX as f64))
}

#[allow(dead_code)]
/// Generate a key from a password/salt using PBKDF2/HMAC/SHA1.
pub fn pbkdf2_sha1(pass: &str, salt: &[u8], iter: usize, keylen: usize) -> CResult<Vec<u8>> {
    Ok(pkcs5::pbkdf2_hmac_sha1(pass, salt, iter, keylen))
}

#[allow(dead_code)]
/// Generate a key from a password/salt using PBKDF2/HMAC/SHA256.
pub fn pbkdf2_sha256(pass: &str, salt: &[u8], iter: usize, keylen: usize) -> CResult<Vec<u8>> {
    Ok(pkcs5::pbkdf2_hmac_sha256(pass, salt, iter, keylen))
}

const BLOCK_SIZE: usize = 16;

/// Pad a byte vector using padding of type PadMode. This is mainly about doing
/// AnsiX923 padding (for backwards compat). We also implement PKCS7, although
/// we don't need to really since most crypto libs use it by default...however
/// it's really easy to do so why not throw it in.
fn pad(data: &mut Vec<u8>, pad_mode: PadMode) {
    let mut pad_len = BLOCK_SIZE - (data.len() % BLOCK_SIZE);
    if pad_len == 0 { pad_len = BLOCK_SIZE; }

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
    let last = match data.last() {
        Some(x) => x + 0,
        None => return
    };
    if last > 16 { return; }

    let datalen = data.len();
    data.truncate(datalen - (last as usize));
}

#[allow(dead_code)]
/// Encrypt data using a 256-bit length key via AES-CBC
pub fn aes_cbc_encrypt(key: &[u8], iv: &[u8], data: &[u8], pad_mode: PadMode) -> CResult<Vec<u8>> {
    //Ok(symm::encrypt(symm::Type::AES_256_CBC, key, iv, data))
    let mut data = Vec::from(data);
    let crypter = symm::Crypter::new(symm::Type::AES_256_CBC);
    crypter.init(symm::Mode::Encrypt, key, iv);

    // if we have padding other than PKCS7, turn OFF OpenSSL's padding and use
    // our own.
    match pad_mode {
        PadMode::PKCS7 => (),
        _ => {
            crypter.pad(false);
            pad(&mut data, pad_mode);
        },
    }

    let mut result = crypter.update(data.as_slice());
    let rest = crypter.finalize();
    result.extend(rest.into_iter());
    Ok(result)
}

#[allow(dead_code)]
/// Decrypt data using a 256-bit length key via AES-CBC
pub fn aes_cbc_decrypt(key: &[u8], iv: &[u8], data: &[u8]) -> CResult<Vec<u8>> {
    //Ok(symm::decrypt(symm::Type::AES_256_CBC, key, iv, data))
    let crypter = symm::Crypter::new(symm::Type::AES_256_CBC);
    crypter.init(symm::Mode::Decrypt, key, iv);

    // ALWAYS turn off padding when decrypting. OpenSSL forces PKCS7 down our
    // throat which we don't always want. So instead, we tell it not to deal
    // with padding on decrypt, and we opt to do it ourselves via unpad().
    crypter.pad(false);

    let mut result = crypter.update(data);
    let rest = crypter.finalize();
    result.extend(rest.into_iter());
    // run the unpad
    unpad(&mut result);
    Ok(result)
}

// TODO: aes-gcm-256

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;
    use std::vec::Vec;
    use std::u64;

    /// Grab a random libertarian string.
    ///
    /// For those who are offended by the data used, and wish to become even
    /// more offended, check out the [elsbot comment history](https://www.reddit.com/user/elsbot),
    /// an aggregation of comments and statements from American propertarians
    /// that are just too ridiculously good not to read.
    ///
    /// NOTE: If you reorder these, all hell will break loose. append to the
    /// list, don't insert. many tests depend on specific ordering.
    fn get_string(idx: i64) -> String {
        let strings = vec![
            String::from(r#"I've got no issues helping the poor but that isn't where an income tax is going to go and if anything it would make the poor poorer."#),
            String::from(r#"They wouldn't be forced, that would be their choice. Thankfully, we are out of our industrial revolution, so no one has to make make that choice anymore. However, in third world countries sweatshops are a very good choice, and I would never want to take that choice away from them."#),
            String::from(r#"Is the current trend of what appears to be global warming harmful or beneficial? In the news you hear only the negative side of it. But what about the benefits?"#),
            String::from(r#"My opinion is that I don't know. I don't care enough even to look at the data. I have seen some things that suggest it exists because of the sun and other that it exists because of CO2. I know that shit has been changing for as long as the earth has been. What I don't like is when people say that most or all scientists agree. This is an appeal to authority and should be disregarded. One thing that makes me skeptical of the claims is that the solution is always more state power in some form. I was all ears, then I was told the governments would need more tax money to forgive my sin fix the problem. Riiiiight. I see whats going on here."#),
            String::from(r#"This is a violation of private property rights. They're calling it an issue of zoning. Zoning is communism."#),
            String::from(r#"Fire them, if they resist, stike or occupy your premises, buy an M134 Minigun to revolve the dispute, then hire new workers"#),
            String::from(r#"Age of consent laws were basically created and supported by the Christian Woman's Association and feminism in order to decrease the supply of available sex to men in order to make it easier for older ladies to find a mate."#),
            String::from(r#"...in a libertarian society the existence of a free baby market will bring such 'neglect' down to a minimum."#),
            String::from(r#"If the options available to a person are work or starve, why would you take away the work option? If people are voluntarily choosing to work in a factory under terrible conditions, it means the alternatives available to them are even worse. That work is an opportunity for them to better themselves. Child labour regulations only hampered the development and expansion of the industries that were providing these opportunities."#),
            String::from(r#"I can't seem to find this mythical document called the "social contract" you claim I am party to."#),
            String::from(r#"...the parent should have the legal right not to feed the child, i.e., to allow it to die."#),
            String::from(r#"The problem with your argument is that you are relying on statistics to support your worldview."#),
            String::from(r#"Genghis Khan has done more good for society and the world than any Democracy that has ever existed..."#),
            String::from(r#"In a covenant...among proprietor and community tenants for the purpose of protecting their private property, no such thing as a right to free (unlimited) speech exists, not even to unlimited speech on one's own tenant-property. One may say innumerable things and promote almost any idea under the sun, but naturally no one is permitted to advocate ideas contrary to the very covenant of preserving and protecting private property, such as democracy and communism. There can be no tolerance toward democrats and communists in a libertarian social order. They will have to be physically separated and removed from society."#),
            String::from(r#"Also I don't see why right to food is a thing. It's not a thing naturally. You have the right to get yourself food in nature, but you aren't entitled to it. It's survival of the fittest."#),
            String::from(r#"Should a business owner be forced to provide goods and services to a citizen against their will? That is the basic question here. If a government or any entity is forcing you against your will to use your knowledge, resources, energy, and money, to do something you do not want to do. We have a word for that, it's slavery."#),
            String::from(r#"If the market for security weren't warped by the violent imposition of a feckless monopoly ("police"), then maybe this child wouldn't have had to provide such a crude implementation of security services for his family."#),
            String::from(r#"And I can go on for hours on hundreds of free market solutions that could solve that problem, The statist can only come up with one solution, Make A LAW!!!"#),
            String::from(r#"None of which is to say that the Empire isn't sometimes brutal. In Episode IV, Imperial stormtroopers kill Luke's aunt and uncle and Grand Moff Tarkin orders the destruction of an entire planet, Alderaan. But viewed in context, these acts are less brutal than they initially appear. Poor Aunt Beru and Uncle Owen reach a grisly end, but only after they aid the rebellion by hiding Luke and harboring two fugitive droids. They aren't given due process, but they are traitors."#),
            String::from(r#"You would have made the choice yourself. No one is holding a gun to your head and telling you to quit, either. If a boss wants to sexually harass you, that's their right. If you want to quit, that is your right. The market will solve any problems."#),
            // not elsbot, but i had to throw it in
            String::from(r#"Is there a 'progressive' response to the idea of the minimum wage being immoral because it violates the non-aggression principle?"#),
        ];
        if idx < 0 {
            let rand = match rand_float() {
                Ok(x) => x,
                Err(..) => 0f64,
            };
            let idx = ((strings.len() as f64) * rand).floor() as usize;
            strings[idx].clone()
        } else {
            strings[idx as usize].clone()
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
        let res = to_base64(&Vec::from(get_string(0).as_bytes())).unwrap();
        assert_eq!(res, "SSd2ZSBnb3Qgbm8gaXNzdWVzIGhlbHBpbmcgdGhlIHBvb3IgYnV0IHRoYXQgaXNuJ3Qgd2hlcmUgYW4gaW5jb21lIHRheCBpcyBnb2luZyB0byBnbyBhbmQgaWYgYW55dGhpbmcgaXQgd291bGQgbWFrZSB0aGUgcG9vciBwb29yZXIu");
    }

    #[test]
    fn can_convert_base64_to_bytes() {
        let res = from_base64(&String::from("VGhleSB3b3VsZG4ndCBiZSBmb3JjZWQsIHRoYXQgd291bGQgYmUgdGhlaXIgY2hvaWNlLiBUaGFua2Z1bGx5LCB3ZSBhcmUgb3V0IG9mIG91ciBpbmR1c3RyaWFsIHJldm9sdXRpb24sIHNvIG5vIG9uZSBoYXMgdG8gbWFrZSBtYWtlIHRoYXQgY2hvaWNlIGFueW1vcmUuIEhvd2V2ZXIsIGluIHRoaXJkIHdvcmxkIGNvdW50cmllcyBzd2VhdHNob3BzIGFyZSBhIHZlcnkgZ29vZCBjaG9pY2UsIGFuZCBJIHdvdWxkIG5ldmVyIHdhbnQgdG8gdGFrZSB0aGF0IGNob2ljZSBhd2F5IGZyb20gdGhlbS4=")).unwrap();
        assert_eq!(String::from_utf8(res).unwrap(), get_string(1));
    }

    #[test]
    fn can_sha256() {
        let data = get_string(2);
        let hash = to_hex(&sha256(data.as_bytes()).unwrap()).unwrap();
        assert_eq!(hash, "bb2747436ce21a01d44636f4566e65a60c982dac2f493d2e517089f2d3b03e71");
    }

    #[test]
    fn can_sha512() {
        let data = get_string(3);
        let hash = to_hex(&sha512(data.as_bytes()).unwrap()).unwrap();
        assert_eq!(hash, "c077cf5be30704b119a0cd4b28947f12b02152543030f649f45dd518636831f71d889d7236eb6041dc4f661c8bc823425269a5f798287badb41fb9ecf750490e");
    }

    #[test]
    fn can_hmac_256() {
        let data = get_string(4);
        let keystr = get_string(5);
        let key = keystr.as_bytes();
        let res = to_hex(&hmac_sha256(&key, &data.as_bytes()).unwrap()).unwrap();
        assert_eq!(res, "b1a698ee4ea7105e79723dfbab65912dffa01c822038b24fbf413a587f241f10");
    }

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
        let password = get_string(11);
        let salt = b"czar@turtl.it";
        let iter: usize = 40000;
        let keylen: usize = 32;
        let res = pbkdf2_sha1(&password, salt, iter, keylen).unwrap();
        assert_eq!(to_hex(&res).unwrap(), "679bc18cc5325b54feac36252f8bb91ff47ae7c2a0e512bb09eaed1ac9ff12c7");
    }

    #[test]
    fn can_pbkdf2_sha256() {
        let password = get_string(10);
        let salt = b"czar@turtl.it";
        let iter: usize = 80669;
        let keylen: usize = 32;
        let res = pbkdf2_sha256(&password, salt, iter, keylen).unwrap();
        assert_eq!(to_hex(&res).unwrap(), "c340a8109fa6421844e32b119926fe6d064553609aa30c4939f83da4fe633c5a");
    }

    #[test]
    fn can_aes_cbc_256_encrypt() {
        let plain = get_string(6);
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
        assert_eq!(decstr, get_string(6));
    }

    #[test]
    fn can_aes_cbc_256_encrypt_ansix923() {
        let plain = get_string(6);
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
        assert_eq!(decstr, get_string(6));
    }
}

