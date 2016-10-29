use ::jedi::Value;

use ::models::model::Model;
use ::models::protected::Protected;

protected!{
    pub struct Persona {
        ( user_id: String,
          pubkey: String,
          email: String,
          name: String,
          settings: Value ),
        ( privkey: String ),
        ( /*generating: bool*/ )
    }
}

make_basic_sync_model!(Persona);

