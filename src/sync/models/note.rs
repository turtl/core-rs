use ::sync::models::SyncModel;

pub struct Note {}

impl Note {
    pub fn new() -> Note {
        Note {}
    }
}

impl SyncModel for Note {
    fn get_table(&self) -> &'static str {
        "notes"
    }
}

