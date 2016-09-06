use ::std::collections::HashMap;

use ::error::{TResult, TFutureResult, TError};
use ::crypto;
use ::api::Status;
use ::models::model::Model;
use ::models::protected::Protected;
use ::futures::{self, Future};
use ::turtl::TurtlWrap;
use ::util::json;

protected!{
    pub struct User {
        ( storage: i64 ),
        ( settings: String ),
        (
            auth: Option<String>
            //logged_in: bool,
            //changing_password: bool
        )
    }
}

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16, iterations: usize) -> TResult<Vec<u8>> {
    let key: Vec<u8> = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            try!(crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400))
        },
        1 => {
            let salt = try!(crypto::to_hex(&try!(crypto::sha256(username.as_bytes()))));
            try!(crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations))
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key)
}

/// Generate a user's auth token given some variables or something
fn generate_auth(username: &String, password: &String, version: u16) -> TResult<(Vec<u8>, String)> {
    let key_auth = match version {
        0 => {
            let key = try!(generate_key(&username, &password, version, 0));
            let iv_str = String::from(&username[..]) + "4c281987249be78a";
            let mut iv = Vec::from(iv_str.as_bytes());
            iv.truncate(16);
            let mut user_record = try!(crypto::to_hex(&try!(crypto::sha256(&password.as_bytes()))));
            user_record.push_str(":");
            user_record.push_str(&username[..]);
            let auth = try!(crypto::encrypt_v0(&key, &iv, &user_record));
            (key, auth)
        },
        1 => {
            let key = try!(generate_key(&username, &password, version, 100000));
            let concat = String::from(&password[..]) + &username;
            let iv_bytes = try!(crypto::sha256(concat.as_bytes()));
            let iv_str = try!(crypto::to_hex(&iv_bytes));
            let iv = Vec::from(&iv_str.as_bytes()[0..16]);
            let pw_hash = try!(crypto::to_hex(&try!(crypto::sha256(&password.as_bytes()))));
            let un_hash = try!(crypto::to_hex(&try!(crypto::sha256(&username.as_bytes()))));
            let mut user_record = String::from(&pw_hash[..]);
            user_record.push_str(":");
            user_record.push_str(&un_hash[..]);
            let utf8_byte: u8 = try!(u8::from_str_radix(&user_record[18..20], 16));
            // have to do a stupid conversion here because of stupidity in the
            // original turtl code. luckily there will be a v2 gen_auth...
            let utf8_random: u8 = (((utf8_byte as f64) / 256.0) * 128.0).floor() as u8;
            let op = try!(crypto::CryptoOp::new_with_iv_utf8("aes", "gcm", iv, utf8_random));
            let auth_bin = try!(crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op));
            let auth = try!(crypto::to_base64(&auth_bin));
            (key, auth)

        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key_auth)
}

/// A worthless function that doesn't do much of anything except keeps the
/// compiler from bitching about all my unused crypto code.
fn use_code(username: &String, password: &String) -> TResult<()> {
    let mut user = User::new();
    user.bind("growl", |_| {
        println!("user growl...");
    }, "user:growl");
    user.bind_once("growl", |_| {
        println!("user growl...");
    }, "user:growl");
    user.unbind("growl", "user:growl");
    user.set("logged_in", true).unwrap();
    user.set("changing_password", false).unwrap();
    let key = try!(crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000));
    let key2 = try!(crypto::random_key());
    let auth = try!(crypto::encrypt_v0(&key, &try!(crypto::random_iv()), &String::from("message")));
    user.auth = Some(auth);
    let auth2 = try!(crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), try!(crypto::CryptoOp::new("aes", "gcm"))));
    let test = String::from_utf8(try!(crypto::decrypt(&key2, &auth2.clone())));
    trace!("debug stuff: {:?}", (auth2, test));
    Ok(())
}

/// A function that tries authenticating a username/password against various
/// versions, starting from latest to earliest until it runs out of versions or
/// we get a match.
fn try_auth(turtl: TurtlWrap, username: String, password: String, version: u16) -> TFutureResult<()> {
    debug!("user::try_auth() -- trying auth version {}", &version);
    let turtl1 = turtl.clone();
    let turtl2 = turtl.clone();
    let ref work = turtl.read().unwrap().work;
    let username_clone = String::from(&username[..]);
    let password_clone = String::from(&password[..]);
    work.run(move || generate_auth(&username_clone, &password_clone, version))
        .and_then(move |key_auth: (Vec<u8>, String)| -> TFutureResult<()> {
            let (key, auth) = key_auth;
            let mut data = HashMap::new();
            data.insert("auth", String::from(&auth[..]));
            let turtl2 = turtl1.clone();
            let ref mut api = turtl1.write().unwrap().api;
            match api.set_auth(String::from(&auth[..])) {
                Err(e) => return futures::done::<(), TError>(Err(e)).boxed(),
                _ => (),
            }
            api.post("/auth", json::to_val(&()))
                .map(move |_| {
                    let ref mut user = turtl2.write().unwrap().user;
                    user.do_login(key, auth);
                })
                .boxed()
        })
        .or_else(move |err| {
            // return with the error value if we hav eanything other than
            // api::Status::Unauthorized
            debug!("user::try_auth() -- api error: {}", err);
            let mut test_err = match err {
                TError::ApiError(x) => {
                    match x {
                        Status::Unauthorized => Ok(()),
                        _ => Err(()),
                    }
                },
                _ => Err(())
            };
            // if we're already at version 0, then JUST FORGET IT
            if version <= 0 { test_err = Err(()); }

            if test_err.is_err() {
                return futures::failed(err).boxed();
            }
            // try again, lower version num
            try_auth(turtl2, username, password, version - 1)
        })
        .boxed()
}

impl User {
    /// Given a turtl, a username, and a password, see if we can log this user
    /// in.
    pub fn login(turtl: TurtlWrap, username: &String, password: &String) -> TFutureResult<()> {
        // -------------------------
        // TODO: removeme
        if password == "get a job get a job get a job omgLOOOOLLLolololLOL" {
            let ref work = turtl.read().unwrap().work;
            work.run(|| use_code(&String::from("ass"), &String::from("butt")));
        }
        // -------------------------

        try_auth(turtl, String::from(&username[..]), String::from(&password[..]), 1)
    }

    /// We have a successful key/auth pair. Log the user in.
    pub fn do_login(&mut self, key: Vec<u8>, auth: String) {
        self.key = Some(key);
        self.auth = Some(auth);
        self.trigger("login", &json::to_val(&()));
    }
}

