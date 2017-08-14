use ::error::{TResult, TError};
use ::models::model::Model;
use ::models::board::Board;
use ::models::note::Note;
use ::models::invite::Invite;
use ::models::protected::{Keyfinder, Protected};
use ::sync::sync_model::{self, SyncModel, MemorySaver};
use ::turtl::Turtl;
use ::lib_permissions::{Role, Permission};

/// Holds information about a member of a space.
#[derive(Serialize, Deserialize, Debug)]
pub struct SpaceMember {
    /// Member id
    pub id: u64,
    /// Member's user_id
    #[serde(with = "::util::ser::int_converter")]
    pub user_id: String,
    /// The space_id this member belongs to
    pub space_id: String,
    /// The email of this member
    pub username: String,
    /// The role of this member
    role: Role,
    /// The permissions this member has
    #[serde(default)]
    permissions: Vec<Permission>,
    /// When the membership was created
    created: String,
    /// When the membership was last updated
    updated: String,
}

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
        let user_id = {
            let isengard = turtl.user_id.read().unwrap();
            match *isengard {
                Some(ref id) => id.clone(),
                None => return Err(TError::MissingField(String::from("Space.permissions_check() -- turtl.user_id is None (not logged in??)"))),
            }
        };

        let err = Err(TError::PermissionDenied(format!("Space::permission_check() -- user {} cannot {:?} on space {}", user_id, permission, space_id)));
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
}

