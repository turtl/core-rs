use ::jedi::Value;
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
    fn default() -> SyncAction { SyncAction::Edit }
}

/// A helpful struct for dealing with sync errors
#[derive(Serialize, Deserialize)]
pub struct SyncError {
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
        pub ty: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub sync_ids: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub missing: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub data: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub error: Option<SyncError>,
    }
}
make_storable!(SyncRecord, "sync_outgoing");
make_basic_sync_model!(SyncRecord);
impl Keyfinder for SyncRecord {}

