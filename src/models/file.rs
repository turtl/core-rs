use ::jedi::Value;

use ::models::model::Model;
use ::models::protected::Protected;

protected!{
    pub struct File {
        ( size: u64,
          has_data: bool ),
        ( name: String,
          type_: String,
          meta: Value ),
        ( )
    }
}

make_basic_sync_model!(File);

