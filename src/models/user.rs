use ::std::collections::HashMap;
use ::jedi::{self, Value, Serialize};
use ::error::{TResult, TError};
use ::crypto::{self, Key};
use ::api::Status;
use ::models::model::Model;
use ::models::space::Space;
use ::models::board::Board;
use ::models::protected::{Keyfinder, Protected};
use ::models::sync_record::{SyncAction, SyncRecord};
use ::turtl::Turtl;
use ::api::ApiReq;
use ::util;
use ::util::event::Emitter;
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::sync::incoming::SyncIncoming;
use ::messaging;

pub const CURRENT_AUTH_VERSION: u16 = 0;

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct User {
        #[serde(skip)]
        pub auth: Option<String>,
        #[serde(skip)]
        pub logged_in: bool,

        #[protected_field(public)]
        pub username: String,

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
impl SyncModel for User {
    // handle change-password syncs
    fn skip_incoming_sync(&self, sync_item: &SyncRecord) -> TResult<bool> {
        Ok(sync_item.action == SyncAction::ChangePassword)
    }
}

impl Keyfinder for User {}

impl MemorySaver for User {
    fn mem_update(self, turtl: &Turtl, action: SyncAction) -> TResult<()> {
        match action {
            SyncAction::Add | SyncAction::Edit => {
                // NOTE: it's not wise to do a direct edit here (as in, lock
                // Turtl.user) because there are many cases when Turtl.user is
                // already locked when we get here. so instead, we blast out an
                // app event that tells us to edit the user object with the data
                // we have.
                messaging::app_event("user:edit", &self.data()?)?;
            }
            SyncAction::Delete => {
                match messaging::ui_event("user:delete", &()) {
                    Ok(_) => (),
                    Err(e) => error!("User.mem_update() -- problem sending `user:delete` event: {}", e),
                }
                turtl.wipe_user_data()?;
            }
            SyncAction::ChangePassword => {
                messaging::app_event("user:change-password:logout", &jedi::obj())?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16) -> TResult<Key> {
    let key: Key = match version {
        0 => {
            let hashme = format!("v{}/{}", version, username);
            let salt = crypto::sha512(hashme.as_bytes())?;
            crypto::gen_key(password.as_bytes(), &salt[0..crypto::KEYGEN_SALT_LEN], crypto::KEYGEN_OPS_DEFAULT, crypto::KEYGEN_MEM_DEFAULT)?
        },
        _ => return TErr!(TError::NotImplemented),
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
        _ => return TErr!(TError::NotImplemented),
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
    let id_err = TErr!(TError::BadValue(format!("auth was successful, but API returned strange id object: {:?}", user_id)));
    let user_id = match user_id {
        Value::Number(x) => {
            match x.as_i64() {
                Some(id) => id.to_string(),
                None => return id_err,
            }
        },
        Value::String(x) => x,
        _ => return id_err,
    };
    let url = format!("/users/{}", user_id);
    user_guard_w.id = Some(user_id);
    user_guard_w.do_login(key, auth);
    drop(user_guard_w);
    let userdata = turtl.api.get(url.as_str(), ApiReq::new())?;
    let mut user_guard = turtl.user.write().unwrap();
    user_guard.merge_fields(&userdata)?;
    user_guard.trigger("login", &jedi::obj());
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
                let e = e.shed();
                match e {
                    TError::Api(x, y) => {
                        match x {
                            // if we got a BAD LOGIN error, try again with a
                            // different (lesser) auth version
                            Status::Unauthorized => {
                                if version <= 0 {
                                    TErr!(TError::Api(Status::Unauthorized, y))
                                } else {
                                    User::login(turtl, username, password, version - 1)
                                }
                            },
                            _ => TErr!(TError::Api(x, y)),
                        }
                    },
                    _ => Err(e)
                }
            })
    }

    pub fn join(turtl: &Turtl, username: String, password: String) -> TResult<()> {
        let (key, auth) = generate_auth(&username, &password, CURRENT_AUTH_VERSION)?;
        let (pk, sk) = crypto::asym::keygen()?;
        let userdata = {
            let mut user = User::default();
            user.set_key(Some(key.clone()));
            user.username = username.clone();
            user.pubkey = Some(pk);
            user.privkey = Some(sk);
            Protected::serialize(&mut user)?
        };

        turtl.api.set_auth(username.clone(), auth.clone())?;
        let mut req = ApiReq::new();

        req = req.data(json!({
            "auth": auth.clone(),
            "username": username,
            "data": userdata,
        }));
        let joindata = turtl.api.post("/users", req)?;
        let user_id: String = jedi::get(&["id"], &joindata)?;
        let user_id: String = user_id.to_string();
        let mut user_guard_w = turtl.user.write().unwrap();
        user_guard_w.merge_fields(jedi::walk(&["data"], &joindata)?)?;
        user_guard_w.id = Some(user_id);
        user_guard_w.storage_mb = jedi::get(&["storage_mb"], &joindata)?;
        user_guard_w.do_login(key, auth);
        drop(user_guard_w);

        let user_guard_r = turtl.user.read().unwrap();
        user_guard_r.trigger("login", &jedi::obj());
        drop(user_guard_r);
        debug!("user::join() -- auth success, joined and logged in");
        Ok(())
    }

    /// Change the current user's password.
    ///
    /// We do this by creating a new user object, generating a key/auth for it,
    /// using that user's new key to re-encrypt the entire in-memory keychain,
    /// then senting the new username, new auth, and new keychain over the to
    /// API in one bulk post.
    ///
    /// The idea is that this is all or nothing. In previous versions of Turtl
    /// we tried to shoehorn this through the sync system, but this tends to be
    /// a delicate procedure and you really want everything to work or nothing.
    pub fn change_password(&mut self, turtl: &Turtl, current_username: String, current_password: String, new_username: String, new_password: String) -> TResult<()> {
        let user_id = self.id_or_else()?;
        let (_, auth) = generate_auth(&current_username, &current_password, CURRENT_AUTH_VERSION)?;
        if Some(auth) != self.auth {
            return TErr!(TError::BadValue(String::from("invalid current username/password given")));
        }

        let mut new_user = self.clone()?;
        new_user.username = new_username;
        let (new_key, new_auth) = generate_auth(&new_user.username, &new_password, CURRENT_AUTH_VERSION)?;
        new_user.set_key(Some(new_key.clone()));
        let new_userdata = Protected::serialize(&mut new_user)?;

        let encrypted_keychain = {
            let profile_guard = turtl.profile.read().unwrap();
            let mut new_keys = Vec::with_capacity(profile_guard.keychain.entries.len());
            for entry in &profile_guard.keychain.entries {
                let mut new_entry = entry.clone()?;
                new_entry.set_key(Some(new_key.clone()));
                let entrydata = Protected::serialize(&mut new_entry)?;
                new_keys.push(entrydata);
            }
            new_keys
        };

        #[derive(Deserialize, Debug)]
        struct PWChangeResponse {
            #[serde(default)]
            #[serde(deserialize_with = "::util::ser::opt_vec_str_i64_converter::deserialize")]
            sync_ids: Option<Vec<i64>>,
        }
        let auth_change = json!({
            "user": new_userdata,
            "auth": new_auth,
            "keychain": encrypted_keychain,
        });
        let url = format!("/users/{}", user_id);
        let res: PWChangeResponse = turtl.api.put(&url[..], ApiReq::new().data(auth_change))?;
        match res.sync_ids.as_ref() {
            Some(ids) => {
                let mut db_guard = turtl.db.write().unwrap();
                match db_guard.as_mut() {
                    Some(db) => SyncIncoming::ignore_on_next(db, ids)?,
                    None => return TErr!(TError::MissingField(String::from("Turtl.db"))),
                }
            }
            None => {}
        }

        turtl.api.set_auth(new_user.username.clone(), new_auth.clone())?;
        turtl.api.post::<String>("/auth", ApiReq::new())?;
        self.do_login(new_key.clone(), new_auth);
        sync_model::save_model(SyncAction::Edit, turtl, self, true)?;

        // save the user's new key into the keychain entries
        {
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut db_guard = turtl.db.write().unwrap();
            let db = match (*db_guard).as_mut() {
                Some(x) => x,
                None => return TErr!(TError::MissingField(format!("Turtl.db"))),
            };
            let user_id = turtl.user_id()?;
            for entry in &mut profile_guard.keychain.entries {
                entry.set_key(Some(new_key.clone()));
                // NOTE: sync_model::save_model() will call mem_update() on our
                // keychain entry, which is bad because that locks the profile
                // (which, as you can see above, is already locked).
                //
                // we kind of side-step syncing here by just directly calling our
                // heroic outgoing() function which saves the object in the db for
                // us. this is pretty much all we'd need save_model() for anyway, so
                // why give it the satisfaction of deadlocking the app?
                entry.outgoing(SyncAction::Edit, &user_id, db, true)?;
            }
        }
        util::sleep(3000);
        Ok(())
    }

    /// Once the user has joined, we set up a default profile for them.
    pub fn post_join(turtl: &Turtl) -> TResult<()> {
        let user_id = {
            let user_guard = turtl.user.read().unwrap();
            user_guard.id_or_else()?
        };

        fn save_space(turtl: &Turtl, user_id: &String, title: &str, color: &str) -> TResult<String> {
            let mut space: Space = Default::default();
            space.generate_key()?;
            space.user_id = user_id.clone();
            space.title = Some(String::from(title));
            space.color = Some(String::from(color));
            let val = sync_model::save_model(SyncAction::Add, turtl, &mut space, false)?;
            let id: String = jedi::get(&["id"], &val)?;
            Ok(id)
        }
        fn save_board(turtl: &Turtl, user_id: &String, space_id: &String, title: &str) -> TResult<String> {
            let mut board: Board = Default::default();
            board.generate_key()?;
            board.user_id = user_id.clone();
            board.space_id = space_id.clone();
            board.title = Some(String::from(title));
            let val = sync_model::save_model(SyncAction::Add, turtl, &mut board, false)?;
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
        let id = {
            let user_guard = turtl.user.read().unwrap();
            user_guard.id_or_else()?
        };
        turtl.api.delete::<bool>(format!("/users/{}", id).as_str(), ApiReq::new())?;
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
        if self.settings.is_none() {
            self.settings = Some(Default::default());
        }
        match self.settings.as_mut() {
            Some(ref mut settings) => {
                settings.insert(String::from(key), jedi::to_val(val)?);
            },
            None => {
                return TErr!(TError::MissingField(String::from("User.settings")));
            }
        }
        sync_model::save_model(SyncAction::Edit, turtl, self, false)?;
        Ok(())
    }

    /// Given an email address, find a matching user (pubkey and all)
    pub fn find_by_email(turtl: &Turtl, email: &String) -> TResult<User> {
        let url = format!("/users/email/{}", email);
        turtl.api.get(url.as_str(), ApiReq::new())
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
