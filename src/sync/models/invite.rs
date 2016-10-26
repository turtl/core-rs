use ::sync::models::SyncModel;

pub struct Invite {}

impl Invite {
    pub fn new() -> Invite {
        Invite {}
    }
}

impl SyncModel for Invite {
    fn get_table(&self) -> &'static str {
        "invites"
    }
}

