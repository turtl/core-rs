use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

/// Let's us know whether a file sync record is incoming or outgoing
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FileSyncType {
    #[serde(rename = "incoming")]
    Incoming,
    #[serde(rename = "outgoing")]
    Outgoing,
}

impl Default for FileSyncType {
    // incoming, right?
    fn default() -> Self { FileSyncType::Incoming }
}

/// Used to notify the incoming/outgoing file sync system of a file that needs
/// to be synced.
protected! {
    #[derive(Serialize, Deserialize)]
    pub struct FileSync {
        #[serde(rename = "type")]
        #[protected_field(public)]
        pub ty: FileSyncType,
    }
}

make_storable!(FileSync, "file_sync");
make_basic_sync_model!(FileSync);
impl Keyfinder for FileSync {}

