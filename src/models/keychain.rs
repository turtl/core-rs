use ::models::model::Model;
use ::models::protected::Protected;

protected!{
    pub struct Keychain {
        ( type_: String,
          item_id: String,
          user_id: String ),
        ( k: Vec<u8> ),
        ( )
    }
}

make_basic_sync_model!(Keychain);

