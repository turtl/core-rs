use ::jedi::Value;

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

make_basic_sync_model!(File);

impl Keyfinder for FileData {}

