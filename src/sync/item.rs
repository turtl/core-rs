//! Here we define an object that acts as the standard container for syncing
//! data between the app and the API.

use ::jedi::Value;

/// Defines a sync item, the standard format used to sync data between the
/// API and the app.
#[derive(Serialize, Deserialize, Debug)]
pub struct SyncItem {
    pub id: String,
    pub action: String,
    pub type_: String,
    pub user_id: Option<String>,
    pub missing: Option<bool>,
    pub data: Option<Value>,
}

