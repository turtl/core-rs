use ::jedi::{self, Value};
use ::error::TResult;
use ::storage::Storage;
use ::models::model::Model;
use ::models::sync_record::SyncRecord;
use ::models::protected::{Keyfinder, Protected};

protected! {
    /// Defines the object we find inside of Note.File (a description of the
    /// note's file with no actual file data...name, mime type, etc).
    #[derive(Serialize, Deserialize)]
    pub struct File {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(public)]
        pub size: Option<u64>,
        #[serde(default)]
        #[protected_field(public)]
        pub has_data: i8,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub name: Option<String>,
        #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub type_: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub meta: Option<Value>,
    }
}

protected! {
    /// Defines the object that holds actual file body data separately from the
    /// metadata that lives in the Note object.
    #[derive(Serialize, Deserialize)]
    pub struct FileData {
        #[serde(default)]
        #[protected_field(public)]
        pub has_data: bool,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub data: Option<Vec<u8>>,
    }
}

make_storable!(FileData, "files");
make_basic_sync_model!{ FileData,
    fn transform(&self, mut sync_item: SyncRecord) -> TResult<SyncRecord> {
        let note_id: String = jedi::get(&["id"], sync_item.data.as_ref().unwrap())?;
        sync_item.data = Some(jedi::get(&["file"], sync_item.data.as_ref().unwrap())?);
        match sync_item.data.as_mut().unwrap() {
            &mut Value::Object(ref mut hash) => {
                hash.remove(&String::from("body"));
            },
            _ => {},
        }

        if jedi::get_opt::<String>(&["note_id"], sync_item.data.as_ref().unwrap()).is_none() {
            jedi::set(&["note_id"], sync_item.data.as_mut().unwrap(), &note_id)?;
        }

        Ok(sync_item)
    }

    fn db_save(&self, db: &mut Storage) -> TResult<()> {
        db.save(self)
        // TODO: add to file download queue
    }
}

impl Keyfinder for FileData {}

