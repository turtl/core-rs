use ::error::{TResult, TFutureResult, TError};
use ::crypto;
use ::api::Api;
use ::models::protected::Protected;
use ::futures::{self, Future};
use ::turtl::TurtlWrap;

protected!{
    pub struct User {
        ( storage: i64 ),
        ( settings: ::util::json::Value ),
        (
            auth: String
            //logged_in: bool,
            //changing_password: bool
        )
    }
}

impl User {
}

fn generate_key(username: &String, password: &String, version: u16, iterations: usize) -> TResult<Vec<u8>> {
    let key: Vec<u8> = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            try_c!(crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400))
        },
        1 => {
            let salt = try_c!(crypto::to_hex(&try_c!(crypto::sha256(username.as_bytes()))));
            try_c!(crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations))
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key)
}

fn generate_auth(username: String, password: String, version: u16) -> TResult<String> {
    let auth = match version {
        0 => {
            let key = try!(generate_key(&username, &password, version, 0));
            let iv_str = String::from(&username[..]) + "4c281987249be78a";
            let mut iv = Vec::from(iv_str.as_bytes());
            iv.truncate(16);
            let mut user_record = try_c!(crypto::to_hex(&try_c!(crypto::sha256(&password.as_bytes()))));
            user_record.push_str(":");
            user_record.push_str(&username[..]);
            try_c!(crypto::encrypt_v0(&key, &iv, &user_record))
        },
        1 => {
            let key = try!(generate_key(&username, &password, version, 100000));
            let concat = String::from(&password[..]) + &username;
            let iv_bytes = try_c!(crypto::sha256(concat.as_bytes()));
            let iv_str = try_c!(crypto::to_hex(&iv_bytes));
            let iv = Vec::from(&iv_str.as_bytes()[0..16]);
            let pw_hash = try_c!(crypto::to_hex(&try_c!(crypto::sha256(&password.as_bytes()))));
            let un_hash = try_c!(crypto::to_hex(&try_c!(crypto::sha256(&username.as_bytes()))));
            let mut user_record = String::from(&pw_hash[..]);
            user_record.push_str(":");
            user_record.push_str(&un_hash[..]);
            let utf8_byte: u8 = try_t!(u8::from_str_radix(&user_record[18..20], 16));
            // have to do a stupid conversion here because of stupidity in the
            // original turtl code. luckily there will be a v2 gen_auth...
            let utf8_random: u8 = (((utf8_byte as f64) / 256.0) * 128.0).floor() as u8;
            let op = try_c!(crypto::CryptoOp::new_with_iv_utf8("aes", "gcm", iv, utf8_random));
            let auth_bin = try_c!(crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op));
            try_c!(crypto::to_base64(&auth_bin))

        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(auth)
}

/// A worthless function that doesn't do much of anything except keeps the
/// compiler from bitching about all my unused crypto code.
fn use_code(username: &String, password: &String) -> TResult<()> {
    let mut user = User::blank();
    user.set("logged_in", &true).unwrap();
    user.set("changing_password", &false).unwrap();
    let key = try_t!(crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000));
    let key2 = try_t!(crypto::random_key());
    let auth = try_t!(crypto::encrypt_v0(&key, &try_t!(crypto::random_iv()), &String::from("message")));
    user.auth = auth;
    let auth2 = try_t!(crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), try_t!(crypto::CryptoOp::new("aes", "gcm"))));
    let test = String::from_utf8(try_t!(crypto::decrypt(&key2, &auth2.clone())));
    trace!("debug stuff: {:?}", (user.stringify_trusted(), auth2, test));
    Ok(())
}

impl User {
    pub fn login(turtl: TurtlWrap, username: String, password: String) -> TFutureResult<()> {
        let ref work = turtl.read().unwrap().work;
        let turtlc = turtl.clone();
        work.run(move || generate_auth(username, password, 1))
            .and_then(move |auth: String| {
                println!("- user: auth: {}", auth);
                let ref work = turtlc.read().unwrap().work;
                work.run(|| use_code(&String::from("ass"), &String::from("butt")))
            })
            .map(|_| println!("- user: used code"))
            .boxed()
    }
}

