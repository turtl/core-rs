//! Here we define an object that acts as the standard container for syncing
//! data between the app and the API.

use ::jedi::Value;

/// Defines a sync item, the standard format used to sync data between the
/// API and the app.
#[derive(Serialize, Deserialize, Debug)]
pub struct SyncItem {
    id: String,
    action: String,
    type_: String,
    user_id: Option<String>,
    missing: Option<bool>,
    data: Option<Value>,
}

