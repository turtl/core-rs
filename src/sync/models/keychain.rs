use ::sync::models::SyncModel;

pub struct Keychain {}

impl Keychain {
    pub fn new() -> Keychain {
        Keychain {}
    }
}

impl SyncModel for Keychain {
    fn get_table(&self) -> &'static str {
        "keychain"
    }
}

