use ::std::collections::HashMap;
use ::jedi::{self, Value, Serialize, DeserializeOwned};

use ::error::{TResult, TError};
use ::crypto::{self, Key};
use ::api::Status;
use ::models::model::Model;
use ::models::space::Space;
use ::models::board::Board;
use ::models::protected::{Keyfinder, Protected};
use ::turtl::Turtl;
use ::api::ApiReq;
use ::util::event::Emitter;
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::sync::SyncRecord;

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct User {
        #[serde(skip)]
        pub auth: Option<String>,
        #[serde(skip)]
        pub logged_in: bool,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub storage_mb: Option<i64>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub name: Option<String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub pubkey: Option<Key>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub settings: Option<HashMap<String, Value>>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub privkey: Option<Key>,
    }
}

make_storable!(User, "users");
make_basic_sync_model!{ User, 
    fn transform(&self, mut sync_item: SyncRecord) -> TResult<SyncRecord> {
        // make sure we convert integer ids to string ids for the user object
        match sync_item.data.as_mut() {
            Some(ref mut data) => {
                match jedi::get_opt::<i64>(&["id"], data) {
                    Some(id) => {
                        jedi::set(&["id"], data, &id.to_string())?;
                    }
                    None => {}
                }
            }
            None => {}
        }
        Ok(sync_item)
    }
}

impl Keyfinder for User {}

impl MemorySaver for User {}

pub const CURRENT_AUTH_VERSION: u16 = 0;

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
fn do_login(turtl: &Turtl, username: &String, password: &String, version: u16) -> TResult<()> {
    let (key, auth) = generate_auth(username, password, version)?;
    turtl.api.set_auth(username.clone(), auth.clone())?;
    let user_id = turtl.api.post("/auth", ApiReq::new())?;
    let mut user_guard_w = turtl.user.write().unwrap();
    let id_err = Err(TError::BadValue(format!("user::do_login() -- auth was successful, but API returned strange id object: {:?}", user_id)));
    user_guard_w.id = match user_id {
        Value::Number(x) => {
            match x.as_i64() {
                Some(id) => Some(id.to_string()),
                None => return id_err,
            }
        },
        Value::String(x) => Some(x),
        _ => return id_err,
    };
    user_guard_w.do_login(key, auth);
    drop(user_guard_w);
    let user_guard_r = turtl.user.read().unwrap();
    user_guard_r.trigger("login", &jedi::obj());
    drop(user_guard_r);
    debug!("user::do_login() -- auth success, logged in");
    Ok(())
}

impl User {
    /// Given a turtl, a username, and a password, see if we can log this user
    /// in.
    pub fn login(turtl: &Turtl, username: String, password: String, version: u16) -> TResult<()> {
        do_login(turtl, &username, &password, version)
            .or_else(|e| {
                turtl.api.clear_auth();
                match e {
                    TError::Api(x, y) => {
                        match x {
                            // if we got a BAD LOGIN error, try again with a
                            // different (lesser) auth version
                            Status::Unauthorized => {
                                if version <= 0 {
                                    Err(TError::Api(Status::Unauthorized, y))
                                } else {
                                    User::login(turtl, username, password, version - 1)
                                }
                            },
                            _ => Err(TError::Api(x, y)),
                        }
                    },
                    _ => Err(e)
                }
            })
    }

    pub fn join(turtl: &Turtl, username: String, password: String) -> TResult<()> {
        let (key, auth) = generate_auth(&username, &password, CURRENT_AUTH_VERSION)?;
        let (pk, sk) = crypto::asym::keygen()?;
        let mut user_guard_w = turtl.user.write().unwrap();
        user_guard_w.set_key(Some(key.clone()));
        user_guard_w.pubkey = Some(pk);
        user_guard_w.privkey = Some(sk);
        user_guard_w.settings = Some(Default::default());
        let userdata = Protected::serialize(&mut (*user_guard_w))?;
        drop(user_guard_w);

        turtl.api.set_auth(username.clone(), auth.clone())?;
        let mut req = ApiReq::new();

        req = req.data(json!({
            "auth": auth.clone(),
            "username": username,
            "data": userdata,
        }));
        let joindata = turtl.api.post("/users", req)?;
        let user_id: u64 = jedi::get(&["id"], &joindata)?;
        let user_id: String = user_id.to_string();
        let mut user_guard_w = turtl.user.write().unwrap();
        user_guard_w.merge_fields(jedi::walk(&["data"], &joindata)?)?;
        user_guard_w.id = Some(user_id);
        user_guard_w.do_login(key, auth);
        user_guard_w.storage_mb = jedi::get(&["storage_mb"], &joindata)?;
        drop(user_guard_w);

        let user_guard_r = turtl.user.read().unwrap();
        user_guard_r.trigger("login", &jedi::obj());
        drop(user_guard_r);
        debug!("user::join() -- auth success, logged in");
        Ok(())
    }

    /// Once the user has joined, we set up a default profile for them.
    pub fn post_join(turtl: &Turtl) -> TResult<()> {
        let mut user_guard_w = turtl.user.write().unwrap();
        let user_id = match user_guard_w.id() {
            Some(x) => x.clone(),
            None => return Err(TError::MissingData(String::from("user.delete_account() -- user has no id, cannot delete"))),
        };
        sync_model::save_model(turtl, user_guard_w.as_mut())?;
        drop(user_guard_w);

        fn save_space(turtl: &Turtl, user_id: &String, title: &str, color: &str) -> TResult<String> {
            let mut space: Space = Default::default();
            space.generate_key()?;
            space.user_id = user_id.clone();
            space.title = Some(String::from(title));
            space.color = Some(String::from(color));
            let val = sync_model::save_model(turtl, &mut space)?;
            let id: String = jedi::get(&["id"], &val)?;
            Ok(id)
        }
        fn save_board(turtl: &Turtl, user_id: &String, space_id: &String, title: &str) -> TResult<String> {
            let mut board: Board = Default::default();
            board.generate_key()?;
            board.user_id = user_id.clone();
            board.space_id = space_id.clone();
            board.title = Some(String::from(title));
            let val = sync_model::save_model(turtl, &mut board)?;
            let id: String = jedi::get(&["id"], &val)?;
            Ok(id)
        }

        let personal_space_id = save_space(turtl, &user_id, "Personal", "#408080")?;
        save_space(turtl, &user_id, "Work", "#439645")?;
        save_space(turtl, &user_id, "Home", "#800000")?;
        save_board(turtl, &user_id, &personal_space_id, "Bookmarks")?;
        save_board(turtl, &user_id, &personal_space_id, "Photos")?;
        save_board(turtl, &user_id, &personal_space_id, "Passwords")?;

        let mut user_guard_w = turtl.user.write().unwrap();
        user_guard_w.set_setting(turtl, "default_space", &personal_space_id)?;
        drop(user_guard_w);

        Ok(())
    }

    /// Static method to log a user out
    pub fn logout(turtl: &Turtl) -> TResult<()> {
        let mut user_guard = turtl.user.write().unwrap();
        if !user_guard.logged_in {
            return Ok(());
        }
        user_guard.do_logout();
        drop(user_guard);
        let user_guard = turtl.user.read().unwrap();
        user_guard.trigger("logout", &jedi::obj());
        turtl.api.clear_auth();
        Ok(())
    }

    /// Delete the current user
    pub fn delete_account(turtl: &Turtl) -> TResult<()> {
        let mut user_guard = turtl.user.write().unwrap();
        user_guard.do_logout();
        let id = match user_guard.id() {
            Some(x) => x.clone(),
            None => return Err(TError::MissingData(String::from("user.delete_account() -- user has no id, cannot delete"))),
        };

        turtl.api.delete(format!("/users/{}", id).as_str(), ApiReq::new())?;
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

    /// Set a setting into this user's settings object
    pub fn set_setting<T>(&mut self, turtl: &Turtl, key: &str, val: &T) -> TResult<()>
        where T: Serialize
    {
        match self.settings.as_mut() {
            Some(ref mut settings) => {
                settings.insert(String::from(key), jedi::to_val(val)?);
            },
            None => {
                return Err(TError::MissingField(String::from("user.set_setting() -- missing user.settings (None)")));
            }
        }
        sync_model::save_model(turtl, self)?;
        Ok(())
    }

    /// Get a user setting
    pub fn get_setting<T>(&self, key: &str) -> Option<T>
        where T: DeserializeOwned
    {
        match self.settings.as_ref() {
            Some(ref settings) => {
                match settings.get(key) {
                    Some(val) => {
                        match jedi::from_val(val.clone()) {
                            Ok(x) => Some(x),
                            Err(_) => None,
                        }
                    },
                    None => None,
                }
            },
            None => None,
        }
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
