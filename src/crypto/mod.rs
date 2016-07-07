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

#[allow(dead_code)]
const CRYPTO_VERSION: u32 = 5;
#[allow(dead_code)]
const CIPHER_INDEX: [&'static str; 1] = [
    "aes",
];
#[allow(dead_code)]
const BLOCK_INDEX: [&'static str; 2] = [
    "cbc",
    "gcm",
];

/// A 
pub struct CryptoData {
    pub version: u32,
    pub desc: Vec<u8>,
    pub iv: Vec<u8>,
    pub hmac: Option<Vec<u8>>,
    pub ciphertext: Vec<u8>,
}

impl CryptoData {
    pub fn new(version: u32, desc: Vec<u8>, iv: Vec<u8>, hmac: Option<Vec<u8>>, ciphertext: Vec<u8>) -> CryptoData {
        CryptoData {
            version: version,
            desc: desc,
            iv: iv,
            hmac: hmac,
            ciphertext: ciphertext,
        }
    }
}

// TODO: serialize
// TODO: deserialize
// TODO: encrypt
// TODO: decrypt

#[allow(dead_code)]
/// Deserialize a serialized cryptographic message. Basically, each piece of
/// crypto data in Turtl has a header, followed by N bytes of ciphertext in
/// the following format:
///
///     |-2 bytes-| |-1 byte----| |-N bytes-----------| |-16 bytes-| |-N bytes-------|
///     | version | |desc length| |payload description| |    IV    | |ciphertext data|
///
/// - `version` tells us the serialization version. although it will probably
///   not get over 255, it has two bytes just in case. never say never.
/// - `desc` length is the length of the payload description, which may change
///   in length from version to version.
/// - `payload description` tells us what algorithm/format the encryption uses.
///   for instance, it could be AES+CBC, or Twofish+CBC, etc etc. payload
///   description encoding/length may change from version to version.
/// - `IV` is the initial vector of the payload, in binary form
/// - `ciphertext data` is our actual encrypted data.
///
/// Note that in older versions (1 <= v <= 4), we used aes-cbc with 
/// encrypt-then-hmac, so our format was as follows:
///
///     |-2 bytes-| |-32 bytes-| |-1 byte----| |-N bytes-----------| |-16 bytes-| |-N bytes-------|
///     | version | |   HMAC   | |desc length| |payload description| |    IV    | |ciphertext data|
///
/// Since we note use authenticated crypto (ie gcm), this format is no longer
/// used, however *all* old serialization versions need to be supported.
///
/// Also note that we basically skipped versions 1 and 2. There is no detectable
/// data that uses those modes, and they can be safely ignored in both their
/// implementation and their tests, although theoretically they use the exact
/// same format as v4.
pub fn deserialize(serialized: &Vec<u8>) -> CResult<CryptoData> {
    let mut idx: usize = 0;
    let mut hmac: Option<Vec<u8>> = None;
    let version: u32 = ((serialized[idx] as u32) << 8) + (serialized[idx + 1] as u32);
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
        hmac = Some(Vec::from(&serialized[idx..(idx + 32)]));
        idx += 32;
    }

    let desc_length = serialized[idx];
    let desc = &serialized[(idx + 1)..(idx + 1 + (desc_length as usize))];
    idx += (desc_length as usize) + 1;

    let iv = &serialized[idx..(idx + 16)];
    idx += 16;

    let ciphertext = &serialized[idx..];

    Ok(CryptoData::new(version, Vec::from(desc), Vec::from(iv), hmac, Vec::from(ciphertext)))
}

#[allow(dead_code)]
/// Generate a random cryptographic key (256-bit).
pub fn random_key() -> CResult<Vec<u8>> {
    low::rand_bytes(32)
}

#[allow(dead_code)]
/// Generate a random IV for use with encryption. This is a helper to enforce
/// the idea that we should not reuse IVs.
pub fn random_iv() -> CResult<Vec<u8>> {
    low::rand_bytes(16)
}

#[allow(dead_code)]
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
    let iv_base64 = String::from_utf8(Vec::from(&serialized[34..])).unwrap();
    let iv = low::from_hex(&iv_base64).unwrap();
    Ok(CryptoData::new(0, Vec::new(), iv, None, ciphertext))
}

#[cfg(test)]
mod tests {
    //! Tests for our high-level Crypto module interface. Note that many of
    //! these tests come from the [common lisp version of turtl-core](https://github.com/turtl/core-cl),
    //! the predecessor to Rust turtl-core.

    use super::*;

    const TEST_ITERATIONS: usize = 32;

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
    //(test (decryption-test-version0 :depends-on key-tests)
    //  "Test decryption of version 0 against Turtl's tcrypt library."
    //  (let* ((key (key-from-string "WPEpNTwrRE144Y7uLuTmJSIYhc1qoo7OMLvZ3oNwaII="))
    //         (tcrypt-ciphertext (babel:string-to-octets "VFQYqIsxYBVdVxANyHepjXvowy107j+n9t1bQqcSI2E2CGLscFMnuZJW6vxLz75XuBHKn0lhbC5FFL1HDuXa1bjvj9CSQcuOl96DqQzGs6BuBHBtTZDHovuzlaC+J7eanoAydRmCuz5iZgKNLLWgWox9e3HWcwRrbyGwOAq5Cj/7s0cn4lKDE5K9V/+x3EA4LzB6aOekBcJNPKD9LmV7I2yifELN2+OAJC7jcICYZHa6i0KciLBZUxTeYfpM1vZJ4suWLH5ZdTFdT9SUINbi06WGFyJtTOQrqlzIz2LFHctsm/FDuU8r9bwFc4sYbha/Ej80+z3S7Zjfp40Ra5GW71oLyK6NyuZSjbdK/xShybiqzyEhA6hf6ekH4Mfef0SlGYTKTvCx7bNd+pPJa/R+LkT/qGgDDJkyzqejvP7guhk=:ib043a089740f1d5ed086225eb30063ff"))
    //         (tcrypt-plaintext "{\"sort\":15,\"type\":\"link\",\"tags\":[\"programming\",\"css\",\"design\"],\"url\":\"http://nicolasgallagher.com/css-drop-shadows-without-images/\",\"title\":\"CSS drop-shadows without images\",\"text\":\"Perhaps a bit dated, but seems to work well. Check [the demo](http://nicolasgallagher.com/css-drop-shadows-without-images/demo/).\"}")
    //         (turtl-plaintext (babel:octets-to-string (decrypt key tcrypt-ciphertext))))
    //    (is (string= tcrypt-plaintext turtl-plaintext))))
    fn can_decrypt_v0() {
        let data = from_base64(&String::from("AAUCAAFhrjnt2O1fiZnsHwyEOr+Ysx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha")).unwrap();
        let des = deserialize(&data).unwrap();

        let cipher_base64 = to_base64(&Vec::from(des.ciphertext)).unwrap();
        let iv_base64 = to_base64(&Vec::from(des.iv)).unwrap();
        assert_eq!(des.desc, [0, 1]);
        assert_eq!(des.version, 5);
        assert_eq!(cipher_base64, "sx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha");
        assert_eq!(iv_base64, "Ya457djtX4mZ7B8MhDq/mA==");
        assert_eq!(des.hmac, None);
    }

    #[test]
    //(test (decryption-test-version3 :depends-on key-tests)
    //  "Test decryption of version 3 against Turtl's tcrypt library. Also contains a
    //   built-in test for fixing utf8-encoded keys."
    //  (let* ((key (key-from-string "HsKTwqcAdzAXSsK2Z8OaOy4RI8OqKnoUw5Miw7BbJcOUT8KOdcO0JwsO"))
    //         (tcrypt-ciphertext (from-base64 "AAOpckDeBymudt1AnCMpNUWE/3gA53BFCXVfl5eRXR6h2gQAAAAAmF5Li7QHzaJda8AwGom/ZGcFhKUjE9VOot2xxxKgQNop6MOkMq6stbbARt8ltbsVQb8I5wSTddcGUJapB6Spd/O+lZ7neYVBNttIm+kb3mekW4AjSBrNBFGpfqsGzOBp3ZVVpkUBJwlCT3/ZJdUXU9KqFlbHq/1uNesiRtXEugTRM4rtKaWoOvPvFye6msGDIxecdjJjI2tSJv4mCvPqnenPw9HzGGDp6U1s9r/FWtdsGoRfxuDPtIKEzuXm4t4CjxMlx/83fOV7xxE4EneMRTOlRUf8MM0eqNkDqAeDK8YNtOmdJLs3XVXRYGvPvh6eR9WcemLbcliz1gqjEmpc+UTWLrL/XDlbDcCQ2RacvJLoEq6i5ogkMa7XTyjKGhrg"))
    //         (tcrypt-plaintext "{\"type\":\"link\",\"url\":\"http://viget.com/extend/level-up-your-shell-game\",\"title\":\"Level Up Your Shell Game\",\"text\":\"Covers movements, aliases, many shortcuts. Good article.\\n\\nTODO: break out the useful bits into individual notes =]\",\"tags\":[\"bash\",\"shell\"],\"sort\":12}")
    //         (turtl-plaintext (babel:octets-to-string (decrypt key tcrypt-ciphertext))))
    //    (is (string= tcrypt-plaintext turtl-plaintext))))
    fn can_decrypt_v3() {
        let data = from_base64(&String::from("AAUCAAFhrjnt2O1fiZnsHwyEOr+Ysx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha")).unwrap();
        let des = deserialize(&data).unwrap();

        let cipher_base64 = to_base64(&Vec::from(des.ciphertext)).unwrap();
        let iv_base64 = to_base64(&Vec::from(des.iv)).unwrap();
        assert_eq!(des.desc, [0, 1]);
        assert_eq!(des.version, 5);
        assert_eq!(cipher_base64, "sx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha");
        assert_eq!(iv_base64, "Ya457djtX4mZ7B8MhDq/mA==");
        assert_eq!(des.hmac, None);
    }

    #[test]
    //(test (decryption-test-version4 :depends-on key-tests)
    //  "Test decryption of version 4 against Turtl's tcrypt library."
    //  (let* ((key (key-from-string "rYMgzeHsjMupzeUvqBpbsDRO1pBk/JWlJp9EHw3yGPs="))
    //         (tcrypt-ciphertext (from-base64 "AAQRk+ROrg4uNqRIwlAJrPOlTupLliAXexfnZpBt2nsCAAQAAAAA8PxvFb3rlGm50n75m4q7aLkif54G7BMiK1cqOAgKIziV7cN3Hyq+d2DggAkpSjnfcJXDDi60SGM+y0kjLUWOIuq0QVOFVF+c9OlhL6eQ5NsgYAr2ElUatg7jwufGbbCS93vItWssCJ3M5h2PTtaHTLtxhI0IrThkqeQYkV7bvK5tKOvo60Vc4pZ0LdAKfulIp3DJ0tmC15Nab2QVNDrQ35WB0tXZIBnloLIG0AkrBZYE+ig7cYK24QM52Z2sPSSQB33cKVe7U4OOZuS4rXBc1xwAhWKom9NZSMTYg6Ke69H4ZZTILZkkW4Qkgt+yIIJf"))
    //         (tcrypt-plaintext "{\"type\":\"link\",\"url\":\"http://www.youtube.com/watch?v=m_0e9VGO05g\",\"title\":\"M-Seven - Electronic Flip - YouTube\",\"text\":\"![image](http://i1.ytimg.com/vi/m_0e9VGO05g/maxresdefault.jpg)  \\n\",\"tags\":[\"electronic\",\"calm\"]}")
    //         (turtl-plaintext (babel:octets-to-string (decrypt key tcrypt-ciphertext))))
    //    (is (string= tcrypt-plaintext turtl-plaintext))))
    fn can_decrypt_v4() {
        let data = from_base64(&String::from("AAUCAAFhrjnt2O1fiZnsHwyEOr+Ysx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha")).unwrap();
        let des = deserialize(&data).unwrap();

        let cipher_base64 = to_base64(&Vec::from(des.ciphertext)).unwrap();
        let iv_base64 = to_base64(&Vec::from(des.iv)).unwrap();
        assert_eq!(des.desc, [0, 1]);
        assert_eq!(des.version, 5);
        assert_eq!(cipher_base64, "sx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha");
        assert_eq!(iv_base64, "Ya457djtX4mZ7B8MhDq/mA==");
        assert_eq!(des.hmac, None);
    }

    #[test]
    //(test (decryption-test-version5 :depends-on key-tests)
    //  "Test decryption of version 5 against Turtl's tcrypt library."
    //  (let* ((key (key-from-string "js8BsJMw2jeqdB/NoidMhP1MDwxCF+XUYf3b+r0fTXs="))
    //         (tcrypt-ciphertext (from-base64 "AAUCAAFKp4T7iuwnVM6+OY+nM9ZKnxe5sUvA+WvkVfhAFDYUKQRenSi+m1WCGGWZIigqL12fAvE4AA10MGACEjEbHvxGQ45qQcEiVZuqB3EMgijklXjNA+EcvnbvcOFSu3KJ4i1iTbsZH+KORmqoxGsWYXafq/EeejAMV94umfC0Uwl6cuCOav2OcDced5GYHwkd9qSDTR+SJyjgAq33r7ylVQQASa8YUP7jx/FGoT4mzjp0+rNAoyqGYU7gJz4v0ccUfm34ww1eZS1kkWmy33h5Cqi6R7Y0y57ytye2WXjvlHGC2/iglx7sPgemDCmIoIMBVdXDW5sMw2fxmpQph1pyst10+Wbv4DcNtN+fMpseDdmTpbXarGNFqyYul/QXM9WmzUTtjeW3kZxol989l+1WXrr6E5Ctk61NVb98PtRMFuRHYU8kt3cTUE4m0G8PGwK62vkp+2pI6fn3UijOFLYHDpeGbgRiAeBbykHgXrtXfAIpysl/FOl1NzfFz44="))
    //         (tcrypt-plaintext "{\"type\":\"link\",\"url\":\"http://www.baynatives.com/plants/Erysimum-capitatum/\",\"title\":\"Erysimum capitatum Gallery - Bay Natives: The San Francisco Source for Native Plants\",\"text\":\"![image](http://www.baynatives.com/plants/Erysimum-capitatum/03-P4082478__.jpg)  \\n\",\"tags\":[\"backyard\",\"garden\",\"native plants\",\"bay natives\",\"flower\",\"wildflower\"]}")
    //         (turtl-plaintext (babel:octets-to-string (decrypt key tcrypt-ciphertext))))
    //    (is (string= tcrypt-plaintext turtl-plaintext))))
    fn can_decrypt_v5() {
        let data = from_base64(&String::from("AAUCAAFhrjnt2O1fiZnsHwyEOr+Ysx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha")).unwrap();
        let des = deserialize(&data).unwrap();

        let cipher_base64 = to_base64(&Vec::from(des.ciphertext)).unwrap();
        let iv_base64 = to_base64(&Vec::from(des.iv)).unwrap();
        assert_eq!(des.desc, [0, 1]);
        assert_eq!(des.version, 5);
        assert_eq!(cipher_base64, "sx06ZcyOYaATj+jCp0MbxsSPsgT9I63q8rgiM7mpDdseylSP1m79IMIUbRNeRDXb7V8pDBF3JXbYUXtFmPFnxft0pjxLSvDO/UcX8dgfjYTqtH7Zsm3XExRPYm3n1C63oqdXM805RPF6Pqb1iAUWvRDVC/1Y992BBk9pQHw4yvnCwFwVsw8EXKNinSt8DAksJvU2FgMt3sq1+tpcbKEaevNTl6h9ElvoeMR3sROSvW6vGwlrxA8OOaKaTRuPcLtNfM5ue/8GqZMBcWbWS+qBPZBtHJsjDOfLa28gyOha");
        assert_eq!(iv_base64, "Ya457djtX4mZ7B8MhDq/mA==");
        assert_eq!(des.hmac, None);
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
