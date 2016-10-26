use ::sync::models::SyncModel;

pub struct User {}

impl User {
    pub fn new() -> User {
        User {}
    }
}

impl SyncModel for User {
    fn get_table(&self) -> &'static str {
        "user"
    }
}

