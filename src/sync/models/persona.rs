use ::sync::models::SyncModel;

pub struct Persona {}

impl Persona {
    pub fn new() -> Persona {
        Persona {}
    }
}

impl SyncModel for Persona {
    fn get_table(&self) -> &'static str {
        "personas"
    }
}

