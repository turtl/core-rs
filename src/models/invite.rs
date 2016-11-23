use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected!{
    pub struct Invite {
        ( object_id: String,
          perms: i64,
          token_server: String,
          has_passphrase: bool,
          has_persona: bool,
          from: String,
          to: String,
          title: String ),
        ( key: Vec<u8>,
          token: String,
          message: String ),
        ( )
    }
}

make_storable!(Invite, "invites");
make_basic_sync_model!(Invite);

impl Keyfinder for Invite {}

