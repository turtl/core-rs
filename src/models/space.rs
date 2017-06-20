use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Space {
        #[protected_field(public)]
        pub user_id: String,

        #[protected_field(private)]
        pub title: Option<String>
    }
}

make_storable!(Space, "spaces");
make_basic_sync_model!(Space);

impl Keyfinder for Space {}

