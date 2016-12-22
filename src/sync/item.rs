//! Here we define an object that acts as the standard container for syncing
//! data between the app and the API.

use ::jedi::Value;

serializable!{
    /// Defines a sync item, the standard format used to sync data between the
    /// API and the app.
    #[derive(Debug)]
    pub struct SyncItem {
        id: String,
        action: String,
        type_: String,
        user_id: String,
        missing: Option<bool>,
        data: Option<Value>,
    }
}

