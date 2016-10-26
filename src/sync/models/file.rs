use ::sync::models::SyncModel;

pub struct File {}

impl File {
    pub fn new() -> File {
        File {}
    }
}

impl SyncModel for File {
    fn get_table(&self) -> &'static str {
        "files"
    }
}

