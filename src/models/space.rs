use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected!{
    pub struct Space {
        ( user_id: String ),
        ( title: String ),
        ( )
    }
}

make_storable!(Space, "spaces");
make_basic_sync_model!(Space);

impl Keyfinder for Space {}

