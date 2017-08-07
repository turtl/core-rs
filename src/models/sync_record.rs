use ::jedi::{self, Value};
use ::error::{TResult, TError};
use ::models::model::Model;
use ::models::protected::{Protected, Keyfinder};
use ::storage::Storage;
use ::turtl::Turtl;

/// How many times a sync record can fail before it's "frozen"
static MAX_ALLOWED_FAILURES: u32 = 3;

/// Makes sure we only accept certain actions for syncing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SyncAction {
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "delete")]
    Delete,
    #[serde(rename = "change-password")]
    ChangePassword,
}

impl Default for SyncAction {
    // edit, right?
    fn default() -> Self { SyncAction::Edit }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SyncType {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "keychain")]
    Keychain,
    #[serde(rename = "space")]
    Space,
    #[serde(rename = "board")]
    Board,
    #[serde(rename = "note")]
    Note,
    #[serde(rename = "file")]
    File,
    // we could have a type for FileOutgoing, but since almost all syncs that
    // use the `sync` table are outgoing, we can just assume the "File" means
    // "FileOutgoing"
    #[serde(rename = "file:incoming")]
    FileIncoming,
    #[serde(rename = "invite")]
    Invite,
}

impl SyncType {
    pub fn from_string(s: String) -> TResult<Self> {
        let val = Value::String(s);
        Ok(jedi::from_val(val)?)
    }
}

impl Default for SyncType {
    // user? doesn't matter
    fn default() -> Self { SyncType::User }
}

/// A helpful struct for dealing with sync errors
#[derive(Serialize, Deserialize)]
pub struct SyncError {
    #[serde(with = "::util::ser::int_converter")]
    pub code: String,
    pub msg: String,
}

/// Define a container for our sync records
protected! {
    #[derive(Serialize, Deserialize)]
    pub struct SyncRecord {
        #[protected_field(public)]
        pub action: SyncAction,
        #[serde(deserialize_with = "::util::ser::int_converter::deserialize")]
        #[protected_field(public)]
        pub item_id: String,
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,
        #[serde(rename = "type")]
        #[protected_field(public)]
        pub ty: SyncType,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub sync_ids: Option<Vec<u64>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub missing: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub data: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub error: Option<SyncError>,
        #[serde(default)]
        #[protected_field(public)]
        pub errcount: u32,
        #[serde(default)]
        #[protected_field(public)]
        pub frozen: bool,
    }
}
make_storable!(SyncRecord, "sync");
make_basic_sync_model!(SyncRecord);
impl Keyfinder for SyncRecord {}

impl SyncRecord {
    /// Clone the non-data, mostly-important bits of a sync record.
    pub fn clone_shallow(&self) -> Self {
        let mut new: SyncRecord = Default::default();
        new.action = self.action.clone();
        new.item_id = self.item_id.clone();
        new.user_id = self.user_id.clone();
        new.ty = self.ty.clone();
        new
    }

    /// Given a DB and some params, grab all matching sync records
    pub fn find(db: &mut Storage, ty: Option<SyncType>) -> TResult<Vec<SyncRecord>> {
        let mut args = vec![];
        if let Some(x) = ty {
            let ty_string: String = jedi::parse(&jedi::stringify(&x)?)?;
            args.push(ty_string+"|");
        }
        db.find("sync", "sync", &args)
    }

    /// Given a DB, find all sync records matching `frozen` but NOT matching
    /// `not_ty`.
    pub fn allbut(db: &mut Storage, not_ty: &Vec<SyncType>) -> TResult<Vec<SyncRecord>> {
        let syncs = SyncRecord::find(db, None)?
            .into_iter()
            .filter(|x| !not_ty.contains(&x.ty))
            .collect::<Vec<SyncRecord>>();
        Ok(syncs)
    }

    /// Static method for grabbing pending sync items. Mainly for the UI's
    /// own personal amusement (but allows enumerating an interface for
    /// unfreezing or deleting bad sync records).
    pub fn get_all_pending(turtl: &Turtl) -> TResult<Vec<SyncRecord>> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::get_all_pending() -- `turtl.db` is empty"))),
        };
        SyncRecord::find(db, None)
    }

    /// Increment this SyncRecord's errcount. If it's above a magic number, we
    /// mark the sync as failed, which excludes it from further outgoing syncs
    /// until it gets manually shaken/removed.
    pub fn handle_failed_sync(db: &mut Storage, failure: &SyncRecord) -> TResult<()> {
        debug!("SyncRecord::handle_failed_sync() -- handle failure: {:?}", failure);
        let sync_id = match failure.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(format!("SyncRecord::handle_failed_sync() -- missing `failure.id` field"))),
        };
        let sync_record: Option<SyncRecord> = db.get("sync", &sync_id)?;
        match sync_record {
            Some(mut rec) => {
                if rec.errcount > MAX_ALLOWED_FAILURES {
                    rec.frozen = true;
                } else {
                    rec.errcount += 1;
                }
                // save our heroic sync record with our mods (errcount/frozen)
                db.save(&rec)?;
            }
            // already deleted? who knows
            None => {}
        }
        Ok(())
    }

    /// Static method that tells the sync system to unfreeze a sync item so it
    /// gets queued to be included in the next outgoing sync.
    pub fn kick_frozen_sync(turtl: &Turtl, sync_id: &String) -> TResult<()> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::kick_frozen_sync() -- `turtl.db` is empty"))),
        };
        let sync: Option<SyncRecord> = db.get("sync", sync_id)?;
        match sync {
            Some(mut rec) => {
                rec.frozen = false;
                db.save(&rec)?;
            }
            None => {}
        }
        Ok(())
    }

    /// Public/static method for deleting a sync record (probably initiated from
    /// the UI).
    pub fn delete_sync_item(turtl: &Turtl, sync_id: &String) -> TResult<()> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::delete_sync_item() -- `turtl.db` is empty"))),
        };
        let mut sync_record: SyncRecord = Default::default();
        sync_record.id = Some(sync_id.clone());
        db.delete(&sync_record)?;
        Ok(())
    }
}

