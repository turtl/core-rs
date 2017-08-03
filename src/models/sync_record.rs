use ::jedi::{self, Value};
use ::error::TResult;
use ::models::model::Model;
use ::models::protected::{Protected, Keyfinder};

/// Makes sure we only accept certain actions for syncing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SyncAction {
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "delete")]
    Delete,
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
make_storable!(SyncRecord, "sync_outgoing");
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
}

