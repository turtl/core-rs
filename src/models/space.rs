use ::error::{TResult, TError};
use ::models::model::Model;
use ::models::board::Board;
use ::models::note::Note;
use ::models::invite::{Invite, InviteRequest};
use ::models::protected::{Keyfinder, Protected};
use ::models::space_member::SpaceMember;
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::turtl::Turtl;
use ::lib_permissions::Permission;
use ::api::ApiReq;
use ::jedi::{self, Value};
use ::crypto::Key;

/// Defines a Space, which is a container for notes and boards. It also acts as
/// an organization of sorts, allowing multiple members to access the space,
/// each with different permission levels.
protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Space {
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,

        // NOTE: with members/spaces, we don't actually have them listed as
        // public/private because we don't want them gumming up the local DB
        // with their nonsense (the API ignores them anyway), but they are not
        // skipped because we do want to be able to send them to the UI as part
        // of the space.
        #[serde(default)]
        pub members: Vec<SpaceMember>,
        #[serde(default)]
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
    fn save_to_mem(self, turtl: &Turtl) -> TResult<()> {
        let mut profile_guard = turtl.profile.write().unwrap();
        for space in &mut profile_guard.spaces {
            if space.id() == self.id() {
                space.merge_fields(&self.data()?)?;
                return Ok(());
            }
        }
        // if it doesn't exist, push it on
        profile_guard.spaces.push(self);
        Ok(())
    }

    fn delete_from_mem(&self, turtl: &Turtl) -> TResult<()> {
        let mut profile_guard = turtl.profile.write().unwrap();
        let space_id = self.id().unwrap();
        for board in &profile_guard.boards {
            if &board.space_id == space_id {
                sync_model::delete_model::<Board>(turtl, board.id().unwrap(), true)?;
            }
        }

        let notes: Vec<Note> = {
            let db_guard = turtl.db.read().unwrap();
            match *db_guard {
                Some(ref db) => db.find("notes", "space_id", &vec![space_id.clone()])?,
                None => vec![],
            }
        };
        for note in notes {
            let note_id = match note.id() {
                Some(x) => x,
                None => {
                    warn!("Space.delete_from_mem() -- got a note from the local DB with empty `id` field");
                    continue;
                }
            };
            sync_model::delete_model::<Note>(turtl, &note_id, true)?;
        }
        // remove the space from memory
        profile_guard.spaces.retain(|s| s.id() != Some(&space_id));
        Ok(())
    }
}

impl Space {
    /// Given a Turtl, a space_id, and a Permission, check if the current user
    /// has the rights to that permission.
    pub fn permission_check(turtl: &Turtl, space_id: &String, permission: &Permission) -> TResult<()> {
        let user_id = turtl.user_id()?;
        let err = TErr!(TError::PermissionDenied(format!("user {} cannot {:?} on space {}", user_id, permission, space_id)));
        let profile_guard = turtl.profile.read().unwrap();
        let matched = profile_guard.spaces.iter()
            .filter(|space| space.id() == Some(space_id))
            .collect::<Vec<_>>();

        // if no spaces in our profile match the given id, we definitely do not
        // have access
        if matched.len() == 0 { return err; }

        let space = matched[0];
        match space.can_i(&user_id, permission)? {
            true => Ok(()),
            false => err,
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

    /// Find a member by id, if such member exists. OR ELSE.
    fn find_member_or_else<'a>(&'a mut self, member_id: u64) -> TResult<&'a mut SpaceMember> {
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
        // kind of inefficient when we already have the space and could just do
        // a can_i, but i don't want to go through all the work of copying the
        // error here. clean code vs less cpu cycles.
        Space::permission_check(turtl, &space_id, &Permission::SetSpaceOwner)?;
        let url = format!("/spaces/{}/owner/{}", space_id, member_user_id);
        let space_data: Value = turtl.api.put(url.as_str(), ApiReq::new())?;
        self.merge_fields(&space_data)?;
        Ok(())
    }

    /// Edit a space member
    pub fn edit_member(&mut self, turtl: &Turtl, member: &mut SpaceMember) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.edit_member()");
        let space_id = get_field!(self, id);
        Space::permission_check(turtl, &space_id, &Permission::EditSpaceMember)?;

        let mut existing_member = self.find_member_or_else(member.id)?;
        member.edit(turtl, Some(&mut existing_member))?;
        Ok(())
    }

    /// Delete a space member
    pub fn delete_member(&mut self, turtl: &Turtl, member_user_id: &String) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.delete_member()");
        let space_id = get_field!(self, id);
        Space::permission_check(turtl, &space_id, &Permission::DeleteSpaceMember)?;

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
        let user_id = turtl.user_id()?;
        let existing_member = self.find_member_by_user_id_or_else(&user_id)?;
        existing_member.delete(turtl)?;
        Ok(())
    }

    /// Send an invite for this space to an unsuspecting
    pub fn send_invite(&mut self, turtl: &Turtl, invite_request: InviteRequest) -> TResult<()> {
        turtl.assert_connected()?;
        let (user_id, username) = {
            let user_guard = turtl.user.read().unwrap();
            let user_id = match user_guard.id() {
                Some(id) => id.clone(),
                None => return TErr!(TError::MissingField(String::from("Turtl.user.id"))),
            };
            (user_id, user_guard.username.clone())
        };
        model_getter!(get_field, "Space.send_invite()");
        let space_id = get_field!(self, id);
        let space_key = match self.key() {
            Some(k) => k.clone(),
            None => return TErr!(TError::MissingField(String::from("Space.key"))),
        };
        Space::permission_check(turtl, &space_id, &Permission::AddSpaceInvite)?;

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

    /// Accept an invite
    pub fn accept_invite(&mut self, turtl: &Turtl, invite_id: &String, passphrase: Option<String>) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.accept_invite()");
        let space_id = get_field!(self, id);
        let spacedata = {
            let invite = self.find_invite_or_else(invite_id)?;
            {
                let user_guard = turtl.user.read().unwrap();
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
            {
                let mut profile_guard = turtl.profile.write().unwrap();
                profile_guard.keychain.upsert_key_save(turtl, &space_id, &key, &String::from("space"), false)?;
            }
            spacedata
        };
        self.members = jedi::get(&["members"], &spacedata)?;
        self.invites = jedi::get(&["invites"], &spacedata)?;
        Ok(())
    }

    /// Edit a space invite
    pub fn edit_invite(&mut self, turtl: &Turtl, invite: &mut Invite) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.edit_invite()");
        let space_id = get_field!(self, id);
        let invite_id = get_field!(invite, id);
        Space::permission_check(turtl, &space_id, &Permission::EditSpaceInvite)?;

        let mut existing_invite = self.find_invite_or_else(&invite_id)?;
        invite.edit(turtl, Some(&mut existing_invite))?;
        Ok(())
    }

    /// Delete a space invite
    pub fn delete_invite(&mut self, turtl: &Turtl, invite_id: &String) -> TResult<()> {
        turtl.assert_connected()?;
        model_getter!(get_field, "Space.delete_invite()");
        let space_id = get_field!(self, id);
        let username = {
            let user_guard = turtl.user.read().unwrap();
            user_guard.username.clone()
        };
        {
            let existing_invite = self.find_invite_or_else(invite_id)?;
            // only check permissions if the invite isn't to the current user
            if existing_invite.to_user != username {
                Space::permission_check(turtl, &space_id, &Permission::DeleteSpaceInvite)?;
            }
            existing_invite.delete(turtl)?;
        }
        self.invites.retain(|x| x.id() != Some(invite_id));
        Ok(())
    }
}

