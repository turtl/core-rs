//! Holds some of the code for the old user model, mainly pertaining to key and
//! auth tag generation.

use ::crypto::{self, Key};
use ::error::{MResult, MError};

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16, iterations: usize) -> MResult<Key> {
    let key: Key = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400)?
        },
        1 => {
            let salt = crypto::to_hex(&crypto::sha256(username.as_bytes())?)?;
            crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations)?
        },
        _ => return Err(MError::NotImplemented),
    };
    Ok(key)
}

/// Generate a user's auth token given some variables or something
pub fn generate_auth(username: &String, password: &String, version: u16) -> MResult<(Key, String)> {
    info!("user::generate_auth() -- generating v{} auth", version);
    let key_auth = match version {
        0 => {
            let key = generate_key(&username, &password, version, 0)?;
            let iv_str = String::from(&username[..]) + "4c281987249be78a";
            let mut iv = Vec::from(iv_str.as_bytes());
            iv.truncate(16);
            let mut user_record = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            user_record.push_str(":");
            user_record.push_str(&username[..]);
            let auth = crypto::encrypt_v0(&key, &iv, &user_record)?;
            (key, auth)
        },
        1 => {
            let key = generate_key(&username, &password, version, 100000)?;
            let concat = String::from(&password[..]) + &username;
            let iv_bytes = crypto::sha256(concat.as_bytes())?;
            let iv_str = crypto::to_hex(&iv_bytes)?;
            let iv = Vec::from(&iv_str.as_bytes()[0..16]);
            let pw_hash = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            let un_hash = crypto::to_hex(&crypto::sha256(&username.as_bytes())?)?;
            let mut user_record = String::from(&pw_hash[..]);
            user_record.push_str(":");
            user_record.push_str(&un_hash[..]);
            let utf8_byte: u8 = u8::from_str_radix(&user_record[18..20], 16)?;
            // have to do a stupid conversion here because of stupidity in the
            // original turtl code. luckily there will be a v2 gen_auth...
            let utf8_random: u8 = (((utf8_byte as f64) / 256.0) * 128.0).floor() as u8;
            let op = crypto::CryptoOp::new_with_iv_utf8("aes", "gcm", iv, utf8_random)?;
            let auth_bin = crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op)?;
            let auth = crypto::to_base64(&auth_bin)?;
            (key, auth)

        },
        _ => return Err(MError::NotImplemented),
    };
    Ok(key_auth)
}

