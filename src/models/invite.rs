use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Invite {
        #[protected_field(public)]
        pub space_id: String,
        #[protected_field(public)]
        pub from_user_id: String,
        #[protected_field(public)]
        pub from_username: String,
        #[protected_field(public)]
        pub to_user: String,
        #[protected_field(public)]
        pub role: String,
        #[protected_field(public)]
		pub is_passphrase_protected: bool,
        #[protected_field(public)]
		pub is_pubkey_protected: bool,
        #[protected_field(public)]
		pub title: String,

        #[protected_field(private)]
        pub message: Option<String>,    // base64
    }
}

make_storable!(Invite, "invites");
make_basic_sync_model!(Invite);

impl Keyfinder for Invite {}

