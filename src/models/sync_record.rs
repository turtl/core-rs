use ::jedi::Value;
use ::models::model::Model;
use ::models::protected::Protected;

/// Define a container for our sync records
protected! {
    #[derive(Serialize, Deserialize)]
    pub struct SyncRecord {
        #[protected_field(public)]
        pub action: String,
        #[serde(with = "::util::ser::int_converter")]
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
    }
}
make_storable!(SyncRecord, "sync_outgoing");

