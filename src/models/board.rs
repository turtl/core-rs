use ::jedi::Value;

use ::models::model::Model;
use ::models::protected::Protected;

protected!{
    pub struct Board {
        ( user_id: String,
          parent_id: String,
          keys: Value,
          privs: Value,
          meta: Value,
          shared: bool ),
        ( title: String ),
        ( )
    }
}

make_basic_sync_model!(Board);

