use ::jedi::Value;

use ::models::model::Model;
use ::models::protected::Protected;
use ::models::file::File;

protected!{
    pub struct Note {
        ( user_id: String,
          boards: Vec<String>,
          file: File,
          has_file: bool,
          keys: Value,
          mod_: u64 ),
        ( type_: String,
          title: String,
          tags: Vec<String>,
          url: String,
          username: String,
          password: String,
          text: String,
          embed: String,
          color: i64 ),
        ( )
    }
}

make_basic_sync_model!(Note);

