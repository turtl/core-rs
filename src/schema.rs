use serde_json::json;
use jedi::Value;

/// Get the app schema.
pub fn get_schema() -> Value {
    // this schema is fed to our `dumpy` lib, which acts as sort of an
    // IndexedDB, providing a way to just "dump" in objects without having a
    // solidified schema. we are able to "lift" fields out of the objects we
    // store and index them to point at the original object. this gives us the
    // ability to search objects generically (without having to know what fields
    // are in each object pffft). this also makes data upgrades (new tables/new
    // indexes) seamless since the storage system is so generic.
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
        // formerly sync_outgoing, and it mostly is, but also used to queue
        // incoming file downloads
        "sync": {
            "indexes": [
                {"name": "sync", "fields": ["type", "frozen"]}
            ]
        },
        "user": {}
    })
}
