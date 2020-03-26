use serde_json::json;
use crate::error::{TResult, TError};
use crate::models::model::Model;
use crate::models::protected::{Keyfinder, Protected};
use crate::models::sync_record::{SyncRecord, SyncAction};
use crate::models::validate::Validate;
use crate::sync::sync_model::{self, SyncModel, MemorySaver};
use crate::sync::incoming;
use lib_permissions::Role;
use crate::crypto::{self, Key};
use jedi::{self, Value};
use crate::turtl::Turtl;
use crate::profile::Profile;

/// Used as our passphrase for our invites if we don't provide one.
const DEFAULT_INVITE_PASSPHRASE: &'static str = "this is the default passphrase lol";

protected! {
    #[derive(serde_derive::Serialize, serde_derive::Deserialize)]
    pub struct Invite {
        #[protected_field(public)]
        pub space_id: String,
        #[serde(with = "crate::util::ser::int_converter")]
        #[protected_field(public)]
        pub from_user_id: String,
        #[protected_field(public)]
        pub from_username: String,
        #[protected_field(public)]
        pub to_user: String,
        #[protected_field(public)]
        pub role: Role,
        #[protected_field(public)]
        pub is_passphrase_protected: bool,
        #[protected_field(public)]
        pub is_pubkey_protected: bool,
        #[protected_field(public)]
        pub title: String,

        #[serde(with = "crate::util::ser::base64_converter")]
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        #[protected_field(private)]
        pub message: Option<Vec<u8>>,
    }
}

/// An object that makes it easy for the UI to send the information needed to
/// create/send an invite
#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
pub struct InviteRequest {
    pub space_id: String,
    pub to_user: String,
    pub role: Role,
    pub title: String,
    pub their_pubkey: Option<Key>,
    pub passphrase: Option<String>,
}

make_storable!(Invite, "invites");
impl SyncModel for Invite {}
impl Validate for Invite {}
impl Keyfinder for Invite {
    // DON'T try to deserialize on mem update
    fn should_deserialize_on_mem_update(&self) -> bool {
        false
    }
}

impl MemorySaver for Invite {
    fn mem_update(self, turtl: &Turtl, sync_item: &mut SyncRecord) -> TResult<()> {
        let action = sync_item.action.clone();
        match action {
            SyncAction::Add | SyncAction::Edit => {
                let mut profile_guard = lockw!(turtl.profile);
                for invite in &mut profile_guard.invites {
                    if invite.id() == self.id() {
                        invite.merge_fields(&self.data()?)?;
                        sync_item.data = Some(invite.data()?);
                        return Ok(());
                    }
                }
                sync_item.data = Some(self.data()?);
                // if it doesn't exist, push it on
                profile_guard.invites.push(self);
            }
            SyncAction::Delete => {
                let mut profile_guard = lockw!(turtl.profile);
                let invite_id = self.id_or_else()?;
                // remove the invite from memory
                profile_guard.invites.retain(|s| s.id() != Some(&invite_id));
            }
            _ => {}
        }
        Ok(())
    }
}

impl Invite {
    /// Convert an invite request+key into an invite, sealed and ready to send
    pub fn from_invite_request(from_user_id: &String, from_username: &String, space_key: &Key, req: InviteRequest) -> TResult<Self> {
        let InviteRequest { space_id, to_user, role, title, their_pubkey, passphrase } = req;
        if title.trim() == "" {
            return TErr!(TError::MissingField(String::from("title")));
        }
        if !to_user.contains("@") {
            return TErr!(TError::BadValue(String::from("email")));
        }
        let mut invite: Invite = Default::default();
        Model::generate_id(&mut invite)?;
        invite.space_id = space_id;
        invite.from_user_id = from_user_id.clone();
        invite.from_username = from_username.clone();
        invite.to_user = to_user;
        invite.role = role;
        invite.is_passphrase_protected = false;
        invite.is_pubkey_protected = false;
        invite.title = title;
        invite.message = None;
        invite.seal(their_pubkey, passphrase, space_key)?;
        Ok(invite)
    }

    /// Generate a key for this invite. If it's not passphrase-protected, then
    /// we'll use a standard password (basically, publicly readable). Set a
    /// passphrase, folks.
    fn gen_invite_key(&mut self, passphrase: Option<String>) -> TResult<()> {
        let passphrase = match passphrase {
            Some(pass) => pass,
            None => String::from(DEFAULT_INVITE_PASSPHRASE),
        };
        let hash = crypto::sha512("invite salt".as_bytes())?;
        let key = crypto::gen_key(passphrase.as_bytes(), &hash[0..crypto::KEYGEN_SALT_LEN], crypto::KEYGEN_OPS_DEFAULT, crypto::KEYGEN_MEM_DEFAULT)?;
        self.set_key(Some(key));
        Ok(())
    }

    /// Sealed with a kiss
    pub fn seal(&mut self, their_pubkey: Option<Key>, passphrase: Option<String>, space_key: &Key) -> TResult<()> {
        let message = jedi::stringify(&json!({"space_key": space_key}))?;
        let mut message = Vec::from(message.as_bytes());
        if let Some(pubkey) = their_pubkey {
            message = crypto::asym::encrypt(&pubkey, message)?;
            self.is_pubkey_protected = true;
        }
        if passphrase.is_some() {
            self.is_passphrase_protected = true;
        }
        self.message = Some(message);
        // talked to drew about generating a key. sounds good.
        self.gen_invite_key(passphrase)?;
        Protected::serialize(self)?;
        Ok(())
    }

    /// Open a sealed invite
    pub fn open(&mut self, our_pubkey: &Key, our_privkey: &Key, passphrase: Option<String>) -> TResult<()> {
        self.gen_invite_key(passphrase)?;
        self.deserialize()?;
        let message = match self.message.as_ref() {
            Some(x) => x.clone(),
            None => return TErr!(TError::MissingField(String::from("Invite.message"))),
        };
        if self.is_pubkey_protected {
            self.message = Some(crypto::asym::decrypt(our_pubkey, our_privkey, message)?);
        }
        Ok(())
    }

    /// Ship it!
    pub fn send(&self, turtl: &Turtl) -> TResult<()> {
        let url = format!("/spaces/{}/invites", self.space_id);
        let data = self.data_for_storage()?;
        let invite: Value = turtl.api.post(url.as_str())?.json(&data).call()?;
        incoming::ignore_syncs_maybe(turtl, &invite, "Invite.send()");
        Ok(())
    }

    /// Accept this invite
    pub fn accept(&self, turtl: &Turtl) -> TResult<Value> {
        model_getter!(get_field, "Invite.accept()");
        let invite_id = get_field!(self, id);
        let url = format!("/spaces/{}/invites/accepted/{}", self.space_id, invite_id);
        let spacedata: Value = turtl.api.post(url.as_str())?.call()?;
        incoming::ignore_syncs_maybe(turtl, &spacedata, "Invite.accept()");
        Ok(spacedata)
    }

    /// Edit this invite
    pub fn edit(&mut self, turtl: &Turtl, existing_invite: Option<&mut Invite>) -> TResult<()> {
        let invite_data = self.data_for_storage()?;
        model_getter!(get_field, "Invite.edit()");
        let invite_id = get_field!(self, id);
        let url = format!("/spaces/{}/invites/{}", self.space_id, invite_id);
        let saved_data: Value = turtl.api.put(url.as_str())?.json(&invite_data).call()?;
        incoming::ignore_syncs_maybe(turtl, &saved_data, "Invite.edit()");
        match existing_invite {
            Some(x) => { *x = jedi::from_val(saved_data)?; }
            None => {}
        }
        Ok(())
    }

    /// Delete an invite
    pub fn delete(&self, turtl: &Turtl) -> TResult<()> {
        model_getter!(get_field, "Invite.delete()");
        let invite_id = get_field!(self, id);
        let url = format!("/spaces/{}/invites/{}", self.space_id, invite_id);
        let ret: Value = turtl.api.delete(url.as_str())?.call()?;
        incoming::ignore_syncs_maybe(turtl, &ret, "Invite.delete()");
        Ok(())
    }

    /// Delete an invite. This is specifically for a space invitee to delete an
    /// invite that was sent to them.
    pub fn delete_user_invite(turtl: &Turtl, invite_id: &String) -> TResult<()> {
        {
            let mut profile_guard = lockw!(turtl.profile);
            let invite = match Profile::finder(&mut profile_guard.invites, invite_id) {
                Some(i) => i,
                None => return TErr!(TError::MissingData(format!("invite doesn't exist: {}", invite_id))),
            };
            invite.delete(turtl)?;
        }
        sync_model::delete_model::<Invite>(turtl, invite_id, true)?;
        Ok(())
    }
}

