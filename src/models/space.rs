use ::error::{TResult, TError};
use ::models::model::Model;
use ::models::board::Board;
use ::models::note::Note;
use ::models::invite::{Invite, InviteRequest};
use ::models::protected::{Keyfinder, Protected};
use ::models::space_member::SpaceMember;
use ::models::sync_record::SyncAction;
use ::models::keychain;
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::turtl::Turtl;
use ::lib_permissions::Permission;
use ::api::ApiReq;
use ::jedi::{self, Value};
use ::crypto::Key;
use ::messaging;

/// Defines a Space, which is a container for notes and boards. It also acts as
/// an organization of sorts, allowing multiple members to access the space,
/// each with different permission levels.
protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Space {
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,
        #[serde(default)]
        #[protected_field(public)]
        pub members: Vec<SpaceMember>,
        #[serde(default)]
        #[protected_field(public)]
        pub invites: Vec<Invite>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub title: Option<String>,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(Color)]
        pub color: Option<String>,
    }
}

make_storable!(Space, "spaces");
impl SyncModel for Space {}

impl Keyfinder for Space {
    // We definitely want to save space keys to the keychain
    fn add_to_keychain(&self) -> bool {
        true
    }
}

impl MemorySaver for Space {
    fn mem_update(self, turtl: &Turtl, action: SyncAction) -> TResult<()> {
        match action {
            SyncAction::Add | SyncAction::Edit => {
                let mut profile_guard = lockw!(turtl.profile);
                for space in &mut profile_guard.spaces {
                    if space.id() == self.id() {
                        space.merge_fields(&self.data()?)?;
                        return Ok(());
                    }
                }
                // if it doesn't exist, push it on
                profile_guard.spaces.push(self);
            }
            SyncAction::Delete => {
                let space_id = self.id_or_else()?;
                let boards: Vec<Board> = {
                    let db_guard = lock!(turtl.db);
                    match *db_guard {
                        Some(ref db) => db.find("boards", "space_id", &vec![space_id.clone()])?,
                        None => vec![],
                    }
                };
                for board in boards {
                    let board_id = board.id_or_else()?;
                    sync_model::delete_model::<Board>(turtl, &board_id, true)?;
                }

                let notes: Vec<Note> = {
                    let db_guard = lock!(turtl.db);
                    match *db_guard {
                        Some(ref db) => db.find("notes", "space_id", &vec![space_id.clone()])?,
                        None => vec![],
                    }
                };
                for note in notes {
                    let note_id = note.id_or_else()?;
                    sync_model::delete_model::<Note>(turtl, &note_id, true)?;
                }
                // remove the space from memory
                let mut profile_guard = lockw!(turtl.profile);
                profile_guard.spaces.retain(|s| s.id() != Some(&space_id));
            }
            _ => {}
        }
        Ok(())
    }
}

impl Space {
    /// Given a Turtl, a space_id, and a Permission, check if the current user
    /// has the rights to that permission.
    pub fn permission_check(turtl: &Turtl, space_id: &String, permission: &Permission) -> TResult<()> {
        let user_id = turtl.user_id()?;
        let profile_guard = lockr!(turtl.profile);
        let matched = profile_guard.spaces.iter()
            .filter(|space| space.id() == Some(space_id))
            .collect::<Vec<_>>();

        // if no spaces in our profile match the given id, we definitely do not
        // have access
        if matched.len() == 0 {
            return TErr!(TError::PermissionDenied(format!("user {} cannot {:?} on space {} (space is missing)", user_id, permission, space_id)));
        }

        let space = matched[0];
        match space.can_i(&user_id, permission)? {
            true => Ok(()),
            false => TErr!(TError::PermissionDenied(format!("user {} cannot {:?} on space {}", user_id, permission, space_id))),
        }
    }

    /// Checks if a user has the given permission on the current space
    pub fn can_i(&self, user_id: &String, permission: &Permission) -> TResult<bool> {
        // if we're the owner, we can do anything
        if user_id == &self.user_id { return Ok(true); }

        let members = &self.members;
        let me_matches = members.iter()
            .filter(|member| &member.user_id == user_id)
            .collect::<Vec<_>>();

        // i'm NOT a member, bob. DON'T look at my face.
        if me_matches.len() == 0 { return Ok(false); }

        let me = me_matches[0];
        Ok(me.role.can(&permission))
    }

    /// Checks if a user has the given permission on the current space, and if
    /// not, returns an error
    pub fn can_i_or_else(&self, user_id: &String, permission: &Permission) -> TResult<()> {
        model_getter!(get_field, "Space.can_i_or_else()");
        let space_id = get_field!(self, id);
        match self.can_i(user_id, permission) {
            Ok(yesno) => {
                if yesno {
                    Ok(())
                } else {
                    TErr!(TError::PermissionDenied(format!("user {} cannot {:?} on space {}", user_id, permission, space_id)))
                }
            },
            Err(e) => Err(e),
        }
    }

    /// Find a member by id, if such member exists. OR ELSE.
    fn find_member_or_else<'a>(&'a mut self, member_id: i64) -> TResult<&'a mut SpaceMember> {
        let member = self.members.iter_mut()
            .filter(|x| x.id == member_id)
            .next();
        match member {
            Some(x) => Ok(x),
            None => TErr!(TError::NotFound(format!("member {} is not a member of this space", member_id))),
        }
    }

    /// Find a member by user_id, if such member exists. OR ELSE.
    fn find_member_by_user_id_or_else<'a>(&'a mut self, member_user_id: &String) -> TResult<&'a mut SpaceMember> {
        let member = self.members.iter_mut()
            .filter(|x| &x.user_id == member_user_id)
            .next();
        match member {
            Some(x) => Ok(x),
            None => TErr!(TError::NotFound(format!("user {} is not a member of this space", member_user_id))),
        }
    }

    /// Find an invite by id OR ELSE
    fn find_invite_or_else<'a>(&'a mut self, invite_id: &String) -> TResult<&'a mut Invite> {
        let invite = self.invites.iter_mut()
            .filter(|x| x.id() == Some(invite_id))
            .next();
        match invite {
            Some(x) => Ok(x),
            None => TErr!(TError::NotFound(format!("invite {} doesn't exist in this space", invite_id))),
        }
    }

    /// Find a space member by email
    fn find_member_by_email<'a>(&'a mut self, email: &String) -> Option<&'a mut SpaceMember> {
        self.members.iter_mut()
            .filter(|x| &x.username == email)
            .next()
    }

    /// Find an existing invite in this space
    fn find_invite_by_email<'a>(&'a mut self, email: &String) -> Option<&'a mut Invite> {
        self.invites.iter_mut()
            .filter(|x| &x.to_user == email)
            .next()
    }

    /// The high council has spoken. This space will have a new owner.
    pub fn set_owner(&mut self, turtl: &Turtl, member_user_id: &String) -> TResult<()> {
        turtl.assert_connected()?;
        // make sure SHE exists
        self.find_member_by_user_id_or_else(member_user_id)?;
        model_getter!(get_field, "Space.set_owner()");
        let space_id = get_field!(self, id);
        let user_id = turtl.user_id()?;
        self.can_i_or_else(&user_id, &Permission::SetSpaceOwner)?;
        let url = format!("/spaces/{}/owner/{}", space_id, member_user_id);
        let space_data: Value = turtl.api.put(url.as_str(), ApiReq::new())?;
        self.merge_fields(&space_data)?;
        Ok(())
    }

    /// Edit a space member
    pub fn edit_member(&mut self, turtl: &Turtl, member: &mut SpaceMember) -> TResult<()> {
        turtl.assert_connected()?;
        let user_id = turtl.user_id()?;
        self.can_i_or_else(&user_id, &Permission::EditSpaceMember)?;

        let mut existing_member = self.find_member_or_else(member.id)?;
        member.edit(turtl, Some(&mut existing_member))?;
        Ok(())
    }

    /// Delete a space member
    pub fn delete_member(&mut self, turtl: &Turtl, member_user_id: &String) -> TResult<()> {
        turtl.assert_connected()?;
        let user_id = turtl.user_id()?;
        self.can_i_or_else(&user_id, &Permission::DeleteSpaceMember)?;

        {
            let existing_member = self.find_member_by_user_id_or_else(member_user_id)?;
            existing_member.delete(turtl)?;
        }
        self.members.retain(|x| &x.user_id != member_user_id);
        Ok(())
    }

    /// Leave the space (as the current user). Like delete, but without a
    /// permission check.
    pub fn leave(&mut self, turtl: &Turtl) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.leave()");
        let space_id = get_field!(self, id);
        let user_id = turtl.user_id()?;
        let existing_member = self.find_member_by_user_id_or_else(&user_id)?;
        existing_member.delete(turtl)?;
        // do the delete async because space deletion requires a profile lock,
        // but it's already locked here.
        messaging::app_event("space:delete", &json!([&space_id, true]))?;
        Ok(())
    }

    /// Send an invite for this space to an unsuspecting
    pub fn send_invite(&mut self, turtl: &Turtl, invite_request: InviteRequest) -> TResult<()> {
        turtl.assert_connected()?;
        let (user_id, username) = {
            let user_guard = lockr!(turtl.user);
            let user_id = user_guard.id_or_else()?;
            (user_id, user_guard.username.clone())
        };
        let space_key = self.key_or_else()?;
        self.can_i_or_else(&user_id, &Permission::AddSpaceInvite)?;

        // if we have an existing member, bail
        if self.find_member_by_email(&invite_request.to_user).is_some() {
            return TErr!(TError::BadValue(format!("{} is already a member of this space", invite_request.to_user)));
        }
        // if we have an existing invite, bail
        if self.find_invite_by_email(&invite_request.to_user).is_some() {
            return TErr!(TError::BadValue(format!("{} is already invited to this space", invite_request.to_user)));
        }

        let invite = Invite::from_invite_request(&user_id, &username, &space_key, invite_request)?;
        invite.send(turtl)?;
        self.invites.push(invite);
        Ok(())
    }

    /// Accept an invite (static)
    pub fn accept_invite(turtl: &Turtl, invite: &mut Invite, passphrase: Option<String>) -> TResult<Space> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.accept_invite()");
        let invite_id = get_field!(invite, id);
        {
            let user_guard = lockr!(turtl.user);
            let pubkey = match user_guard.pubkey.as_ref() {
                Some(k) => k,
                None => return TErr!(TError::MissingField(String::from("User.pubkey"))),
            };
            let privkey = match user_guard.privkey.as_ref() {
                Some(k) => k,
                None => return TErr!(TError::MissingField(String::from("User.privkey"))),
            };
            invite.open(pubkey, privkey, passphrase)?;
        }
        let keyjson = match invite.message.as_ref() {
            Some(data) => jedi::parse(&String::from_utf8(data.clone())?)?,
            None => return TErr!(TError::MissingField(String::from("Invite.message"))),
        };
        let key: Key = jedi::get(&["space_key"], &keyjson)?;
        let spacedata = invite.accept(turtl)?;
        // save the key directly. i'm nowadays fairly paranoid of keys being
        // lost during handoff periods like this, so we save the key once
        // here and once again when we save the space we just got into our
        // local storage
        keychain::save_key(turtl, &invite.space_id, &key, &String::from("space"), false)?;
        let mut space: Space = jedi::from_val(spacedata)?;
        space.set_key(Some(key));
        // make sure we deserialize the space. this should techinically happen
        // in the turtl.work() thread, but honestly i don't have the energy to
        // deal with all the clones when this is already really close to working
        // so for now it's just going to be inlined.
        space.deserialize()?;
        // save the space locally (along with its key, which will be double-
        // saved because we are paranoid).
        sync_model::save_model(SyncAction::Add, turtl, &mut space, true)?;
        // also, remove our invite locally. it's not...economically viable.
        sync_model::delete_model::<Invite>(turtl, &invite_id, true)?;
        Ok(space)
    }

    /// Edit a space invite
    pub fn edit_invite(&mut self, turtl: &Turtl, invite: &mut Invite) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.edit_invite()");
        let user_id = turtl.user_id()?;
        let invite_id = get_field!(invite, id);
        self.can_i_or_else(&user_id, &Permission::EditSpaceInvite)?;

        let mut existing_invite = self.find_invite_or_else(&invite_id)?;
        invite.edit(turtl, Some(&mut existing_invite))?;
        Ok(())
    }

    /// Delete a space invite. This is specifically for a space admin deleting
    /// an invite on the space (in other words, the endpoint for deleting an
    /// invite if you are an inviter, not invitee).
    pub fn delete_invite(&mut self, turtl: &Turtl, invite_id: &String) -> TResult<()> {
        turtl.assert_connected()?;
        let username = {
            let user_guard = lockr!(turtl.user);
            user_guard.username.clone()
        };
        {
            let user_id = turtl.user_id()?;
            let perm_check = self.can_i_or_else(&user_id, &Permission::DeleteSpaceInvite);
            let existing_invite = self.find_invite_or_else(invite_id)?;
            // only check permissions if the invite isn't to the current user
            if existing_invite.to_user != username {
                perm_check?;
            }
            existing_invite.delete(turtl)?;
        }
        self.invites.retain(|x| x.id() != Some(invite_id));
        Ok(())
    }
}

