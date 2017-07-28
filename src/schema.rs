use ::jedi::Value;

/// Get the app schema.
pub fn get_schema() -> Value {
    json!({
        "boards": {
            "indexes": [
                {"fields": ["space_id"]},
                {"fields": ["user_id"]}
            ]
        },
        "invites": {},
        "keychain": {
            "indexes": [
                {"fields": ["item_id"]}
            ]
        },
        "notes": {
            "indexes": [
                {"fields": ["user_id"]},
                {"fields": ["space_id"]},
                {"fields": ["board_id"]},
                {"fields": ["has_file"]}
            ]
        },
        "spaces": {
            "indexes": [
                {"fields": ["user_id"]}
            ]
        },
        "sync_outgoing": {
            "indexes": [
                {"fields": ["frozen"]}
            ]
        },
        "user": {}
    })
}
