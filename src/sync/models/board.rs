use ::sync::models::SyncModel;

pub struct Board {}

impl Board {
    pub fn new() -> Board {
        Board {}
    }
}

impl SyncModel for Board {
    fn get_table(&self) -> &'static str {
        "boards"
    }
}

