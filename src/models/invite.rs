use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Invite {
        #[protected_field(public)]
        space_id: String,
        #[protected_field(public)]
        from_user_id: String,
        #[protected_field(public)]
        from_username: String,
        #[protected_field(public)]
        to_user: String,
        #[protected_field(public)]
        role: String,
        #[protected_field(public)]
		is_passphrase_protected: bool,
        #[protected_field(public)]
		is_pubkey_protected: bool,
        #[protected_field(public)]
		title: String,

        #[protected_field(private)]
        message: Option<String>,    // base64
    }
}

make_storable!(Invite, "invites");
make_basic_sync_model!(Invite);

impl Keyfinder for Invite {}

