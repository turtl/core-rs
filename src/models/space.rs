use ::error::TResult;
use ::models::model::Model;
use ::models::board::Board;
use ::models::note::Note;
use ::models::invite::Invite;
use ::models::protected::{Keyfinder, Protected};
use ::sync::sync_model::{self, MemorySaver};
use ::turtl::Turtl;
use ::lib_permissions::{Role, Permission};

/// Holds information about a member of a space.
#[derive(Serialize, Deserialize, Debug)]
pub struct SpaceMember {
    /// Member id
    pub id: u64,
    /// Member's user_id
    pub user_id: u64,
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
make_basic_sync_model!(Space);

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
                return Ok(())
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

        let db_guard = turtl.db.read().unwrap();
        let notes: Vec<Note> = match *db_guard {
            Some(ref db) => db.find("notes", "space_id", &vec![space_id.clone()])?,
            None => vec![],
        };
        drop(db_guard);
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
        profile_guard.spaces.retain(|s| {
            match s.id() {
                Some(id) => (space_id != id),
                None => true,
            }
        });

        Ok(())
    }
}

