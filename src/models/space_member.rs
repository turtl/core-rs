use ::error::TResult;
use ::lib_permissions::{Role, Permission};
use ::turtl::Turtl;
use ::jedi::{self, Value};
use ::api::ApiReq;
use ::sync::incoming::SyncIncoming;

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
    pub role: Role,
    /// The permissions this member has
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// When the membership was created
    pub created: String,
    /// When the membership was last updated
    pub updated: String,
}

/// Given a Value object with sync_ids, try to ignore the sync ids. Kids' stuff.
fn ignore_syncs_maybe(turtl: &Turtl, val_with_sync_ids: &Value, errtype: &str) {
    match jedi::get_opt::<Vec<u64>>(&["sync_ids"], val_with_sync_ids) {
        Some(x) => {
            let mut db_guard = turtl.db.write().unwrap();
            if db_guard.is_some() {
                match SyncIncoming::ignore_on_next(db_guard.as_mut().unwrap(), &x) {
                    Ok(..) => {},
                    Err(e) => error!("{} -- error ignoring sync items: {}", errtype, e),
                }
            }
        }
        None => {}
    }
}

impl SpaceMember {
    /// Save this item
    pub fn edit(&mut self, turtl: &Turtl, existing_member: Option<&mut SpaceMember>) -> TResult<()> {
        let member_data = jedi::to_val(self)?;
        let url = format!("/spaces/{}/members/{}", self.space_id, self.user_id);
        let saved_data: Value = turtl.api.put(url.as_str(), ApiReq::new().data(member_data))?;
        ignore_syncs_maybe(turtl, &saved_data, "SpaceMember.edit()");
        match existing_member {
            Some(x) => { *x = jedi::from_val(saved_data)?; }
            None => {}
        }
        Ok(())
    }

    /// Delete this member from the space
    pub fn delete(&mut self, turtl: &Turtl) -> TResult<()> {
        let url = format!("/spaces/{}/members/{}", self.space_id, self.user_id);
        let ret: Value = turtl.api.delete(url.as_str(), ApiReq::new())?;
        ignore_syncs_maybe(turtl, &ret, "SpaceMember.delete()");
        Ok(())
    }
}

