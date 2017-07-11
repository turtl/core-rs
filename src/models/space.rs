use ::error::TResult;
use ::models::model::Model;
use ::models::protected::{Keyfinder, Protected};
use ::sync::sync_model::MemorySaver;
use ::turtl::TurtlWrap;

protected! {
    #[derive(Serialize, Deserialize)]
    pub struct Space {
        #[serde(with = "::util::ser::int_converter")]
        #[protected_field(public)]
        pub user_id: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        #[protected_field(private)]
        pub title: Option<String>
    }
}

make_storable!(Space, "spaces");
make_basic_sync_model!(Space);

impl Keyfinder for Space {
    // We definitely want to save space keys to the keychain
    fn add_to_keychain(&self) -> bool {
        true
    }
}

impl MemorySaver for Space {
    fn save_to_mem(self, turtl: TurtlWrap) -> TResult<()> {
        let mut profile_guard = turtl.profile.write().unwrap();
        for space in &mut profile_guard.spaces {
            if space.id() == self.id() {
                space.merge_fields(&self.data()?)?;
                return Ok(())
            }
        }
        // if it doesn't exist, push it on
        profile_guard.spaces.push(self);
        Ok(())
    }
}

