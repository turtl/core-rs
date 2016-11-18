use ::std::collections::HashMap;

use ::jedi::{self, Value};

use ::error::{TResult, TFutureResult, TError};
use ::crypto;
use ::api::Status;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::futures::{self, Future};
use ::turtl::TurtlWrap;
use ::api::ApiReq;
use ::util::event::Emitter;

protected!{
    pub struct User {
        ( storage: i64 ),
        ( settings: String ),
        (
            auth: Option<String>,
            logged_in: bool
        )
    }
}

make_basic_sync_model!(User);

impl Keyfinder for User {}

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16, iterations: usize) -> TResult<Vec<u8>> {
    let key: Vec<u8> = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400)?
        },
        1 => {
            let salt = crypto::to_hex(&crypto::sha256(username.as_bytes())?)?;
            crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations)?
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key)
}

/// Generate a user's auth token given some variables or something
pub fn generate_auth(username: &String, password: &String, version: u16) -> TResult<(Vec<u8>, String)> {
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
    let key = crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000)?;
    let key2 = crypto::random_key()?;
    let auth = crypto::encrypt_v0(&key, &crypto::random_iv()?, &String::from("message"))?;
    user.auth = Some(auth);
    let auth2 = crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), crypto::CryptoOp::new("aes", "gcm")?)?;
    let test = String::from_utf8(crypto::decrypt(&key2, &auth2.clone())?);
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
    let ref work = turtl.work;
    let username_clone = String::from(&username[..]);
    let password_clone = String::from(&password[..]);
    work.run(move || generate_auth(&username_clone, &password_clone, version))
        .and_then(move |key_auth: (Vec<u8>, String)| -> TFutureResult<()> {
            let (key, auth) = key_auth;
            let mut data = HashMap::new();
            data.insert("auth", auth.clone());
            {
                let ref api = turtl1.api;
                match api.set_auth(auth.clone()) {
                    Err(e) => return futures::done::<(), TError>(Err(e)).boxed(),
                    _ => (),
                }
            }
            let turtl4 = turtl1.clone();
            turtl1.with_api(|api| -> TResult<Value> {
                api.post("/auth", ApiReq::new())
            }).and_then(move |user_id| {
                let mut user_guard_w = turtl4.user.write().unwrap();
                user_guard_w.id = match user_id {
                    Value::String(x) => Some(x),
                    _ => return futures::failed(TError::BadValue(format!("user::try_auth() -- auth was successful, but API returned strange id object: {:?}", user_id))).boxed(),
                };
                user_guard_w.do_login(key, auth);
                drop(user_guard_w);
                let user_guard_r = turtl4.user.read().unwrap();
                user_guard_r.trigger("login", &jedi::obj());
                drop(user_guard_r);
                debug!("user::try_auth() -- auth success, logged in");
                futures::finished(()).boxed()
            }).boxed()
        })
        .or_else(move |err| {
            // return with the error value if we have anything other than
            // api::Status::Unauthorized
            debug!("user::try_auth() -- api error: {}", err);
            let mut test_err = match err {
                TError::Api(x) => {
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
            turtl.work.run(|| use_code(&String::from("ass"), &String::from("butt")));
        }
        // -------------------------

        try_auth(turtl, String::from(&username[..]), String::from(&password[..]), 1)
    }

    /// Static method to log a user out
    pub fn logout(turtl: TurtlWrap) -> TResult<()> {
        let mut user_guard = turtl.user.write().unwrap();
        user_guard.do_logout();
        drop(user_guard);
        let user_guard = turtl.user.read().unwrap();
        user_guard.trigger("logout", &jedi::obj());
        Ok(())
    }

    /// We have a successful key/auth pair. Log the user in.
    pub fn do_login(&mut self, key: Vec<u8>, auth: String) {
        self.set_key(Some(key));
        self.auth = Some(auth);
        self.logged_in = true;
    }

    /// Logout the user
    pub fn do_logout(&mut self) {
        self.set_key(None);
        self.auth = None;
        self.logged_in = false;
    }
}

