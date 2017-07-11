use ::std::collections::HashMap;

use ::jedi::{self, Value};

use ::error::{TResult, TFutureResult, TError};
use ::crypto::{self, Key};
use ::api::Status;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::futures::Future;
use ::turtl::TurtlWrap;
use ::api::ApiReq;
use ::util::event::Emitter;
use ::sync::sync_model::MemorySaver;

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct User {
        #[serde(skip)]
        pub auth: Option<String>,
        #[serde(skip)]
        pub logged_in: bool,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub storage: Option<i64>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub settings: Option<String>,
    }
}

make_storable!(User, "users");
make_basic_sync_model!(User);

impl Keyfinder for User {}

impl MemorySaver for User {}

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16) -> TResult<Key> {
    let key: Key = match version {
        0 => {
            let hashme = format!("v{}/{}", version, username);
            let salt = crypto::sha512(hashme.as_bytes())?;
            crypto::gen_key(password.as_bytes(), &salt[0..crypto::KEYGEN_SALT_LEN], crypto::KEYGEN_OPS_DEFAULT, crypto::KEYGEN_MEM_DEFAULT)?
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key)
}

/// Generate a user's auth token given some variables or something
pub fn generate_auth(username: &String, password: &String, version: u16) -> TResult<(Key, String)> {
    info!("user::generate_auth() -- generating v{} auth", version);
    let key_auth = match version {
        0 => {
            let key = generate_key(username, password, version)?;
            let nonce_len = crypto::noncelen();
            let nonce = (crypto::sha512(username.as_bytes())?)[0..nonce_len].to_vec();
            let pw_hash = crypto::to_hex(&crypto::sha512(&password.as_bytes())?)?;
            let user_record = String::from(&pw_hash[..]);
            let op = crypto::CryptoOp::new_with_nonce("chacha20poly1305", nonce)?;
            let auth_bin = crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op)?;
            let auth = crypto::to_hex(&auth_bin)?;
            (key, auth)
        }
        _ => return Err(TError::NotImplemented),
    };
    Ok(key_auth)
}

/// A function that tries authenticating a username/password against various
/// versions, starting from latest to earliest until it runs out of versions or
/// we get a match.
fn try_auth(turtl: TurtlWrap, username: String, password: String, version: u16) -> TFutureResult<()> {
    debug!("user::try_auth() -- trying auth version {}", &version);
    let turtl1 = turtl.clone();
    let turtl2 = turtl.clone();
    let ref work = turtl.work;
    let username_clone = username.clone();
    let password_clone = password.clone();
    let username_api_clone = username.clone();
    work.run(move || generate_auth(&username_clone, &password_clone, version))
        .and_then(move |key_auth: (Key, String)| -> TFutureResult<()> {
            let (key, auth) = key_auth;
            let mut data = HashMap::new();
            data.insert("auth", auth.clone());
            {
                let ref api = turtl1.api;
                match api.set_auth(username_api_clone, auth.clone()) {
                    Err(e) => return FErr!(e),
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
                    _ => return FErr!(TError::BadValue(format!("user::try_auth() -- auth was successful, but API returned strange id object: {:?}", user_id))),
                };
                user_guard_w.do_login(key, auth);
                drop(user_guard_w);
                let user_guard_r = turtl4.user.read().unwrap();
                user_guard_r.trigger("login", &jedi::obj());
                drop(user_guard_r);
                debug!("user::try_auth() -- auth success, logged in");
                FOk!(())
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
                return FErr!(err);
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
        try_auth(turtl, String::from(&username[..]), String::from(&password[..]), 0)
    }

    /*
    pub fn join(turtl: TurtlWrap, username: &String, password: &String) -> TFutureResult<()> {
        FOk!(())
    }
    */

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
    pub fn do_login(&mut self, key: Key, auth: String) {
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

#[cfg(test)]
mod tests {
    //! Tests for our high-level Crypto module interface.

    use super::*;

    #[test]
    pub fn authgen() {
        let username = String::from("andrew@lyonbros.com");
        let password = String::from("slippy");
        let (_key, auth) = generate_auth(&username, &password, 0).unwrap();
        assert_eq!(auth, "000601000c9af06607bbb78b0cab4e01f29a8d06da9a65e5698768b88ac4f4c04002c96fcfcb18a1644d5ba2546901452d0ebd6c162fe494997b52660d9d190ed525076523a1a576ea7596fdaec2e0f0606f3290bd6e5815f76889a4eada71fc20dad21703453928c74db36880cf6035922e3f7093ed1eef01a630750ebd8d64baaf34e325536011de40f3a72a4d95155ca32e851257d8bc7736d2d41c92213e93");
    }
}
