use crate::error::TResult;
use lib_permissions::{Role, Permission};
use crate::turtl::Turtl;
use jedi::{self, Value};
use crate::sync::incoming;

/// Holds information about a member of a space.
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Debug, Default)]
pub struct SpaceMember {
    /// Member id
    #[serde(with = "crate::util::ser::str_i64_converter")]
    pub id: i64,
    /// Member's user_id
    pub user_id: String,
    /// The space_id this member belongs to
    pub space_id: String,
    /// The email of this member
    pub username: String,
    /// The role of this member
    pub role: Role,
    /// The permissions this member has
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// When the membership was created
    pub created: String,
    /// When the membership was last updated
    pub updated: String,
}

impl SpaceMember {
    /// Save this item
    pub fn edit(&mut self, turtl: &Turtl, existing_member: Option<&mut SpaceMember>) -> TResult<()> {
        let member_data = jedi::to_val(self)?;
        let url = format!("/spaces/{}/members/{}", self.space_id, self.user_id);
        let saved_data: Value = turtl.api.put(url.as_str())?.json(&member_data).call()?;
        incoming::ignore_syncs_maybe(turtl, &saved_data, "SpaceMember.edit()");
        match existing_member {
            Some(x) => { *x = jedi::from_val(saved_data)?; }
            None => {}
        }
        Ok(())
    }

    /// Delete this member from the space
    pub fn delete(&self, turtl: &Turtl) -> TResult<()> {
        let url = format!("/spaces/{}/members/{}", self.space_id, self.user_id);
        let ret: Value = turtl.api.delete(url.as_str())?.call()?;
        incoming::ignore_syncs_maybe(turtl, &ret, "SpaceMember.delete()");
        Ok(())
    }
}

