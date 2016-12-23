use ::std::sync::Arc;

use ::jedi::{self, Value};
use ::error::TResult;
use ::storage::Storage;
use ::sync::item::SyncItem;
use ::models::storable::Storable;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected!{
    /// Defines the object we find inside of Note.File (a description of the
    /// note's file with no actual file data...name, mime type, etc).
    pub struct File {
        ( size: u64,
          has_data: bool ),
        ( name: String,
          type_: String,
          meta: Value ),
        ( )
    }
}

protected!{
    /// Defines the object that holds actual file body data separately from the
    /// metadata that lives in the Note object.
    pub struct FileData {
        ( note_id: String,
          has_data: bool ),
        ( data: Vec<u8> ),
        ( )
    }
}

make_storable!(FileData, "files");
make_basic_sync_model!{ FileData,
    fn transform(&self, mut sync_item: SyncItem) -> TResult<SyncItem> {
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

    fn db_save<T>(&self, db: &Arc<Storage>, model: &T) -> TResult<()>
        where T: Protected + Storable
    {
        db.save(model)
        // TODO: add to file download queue
    }
}

impl Keyfinder for FileData {}

