use ::std::sync::{Arc, RwLock, Mutex};

use ::jedi;

use ::error::TResult;
use ::sync::{SyncConfig, Syncer};
use ::sync::incoming::SyncIncoming;
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models::sync_record::{SyncType, SyncRecord};

#[derive(Deserialize, Debug)]
struct SyncResponse {
    /// successful sync records
    #[serde(default)]
    success: Vec<SyncRecord>,
    /// records that failed to sync properly
    #[serde(default)]
    failures: Vec<SyncRecord>,
    /// records that weren't run because they were blocked by a failure
    #[serde(default)]
    blocked: Vec<SyncRecord>,
}

/// Holds the state for data going from turtl -> API (outgoing sync data).
pub struct SyncOutgoing {
    /// Holds our sync config. Note that this is shared between the sync system
    /// and the `Turtl` object in the main thread.
    config: Arc<RwLock<SyncConfig>>,

    /// Holds our Api object. Lets us chit chat with the Turtl server.
    api: Arc<Api>,

    /// Holds our user-specific db. This is mainly for persisting k/v data and
    /// for polling the "outgoing" table for local changes that need to be
    /// synced to our heroic API.
    db: Arc<Mutex<Option<Storage>>>,

    /// Stores our syn run version
    run_version: i64,
}

impl SyncOutgoing {
    /// Create a new outgoing syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<Mutex<Option<Storage>>>) -> SyncOutgoing {
        SyncOutgoing {
            config: config,
            api: api,
            db: db,
            run_version: 0,
        }
    }

    /// Grab all non-file outgoing sync items, in order
    fn get_outgoing_syncs(&self) -> TResult<Vec<SyncRecord>> {
        let syncs = with_db!{ db, self.db,
            SyncRecord::allbut(db, &vec![SyncType::FileOutgoing, SyncType::FileIncoming])
        }?;

        // stop at our first frozen record! this creates a "block" that must be
        // cleared before syncing can continue.
        let mut final_syncs = Vec::with_capacity(syncs.len());
        for sync in syncs {
            if sync.frozen { break; }
            final_syncs.push(sync);
        }
        Ok(final_syncs)
    }

    /// Delete a sync record from sync (like, when we send it to the API and it
    /// runs successfully...we don't need it sitting around).
    fn delete_sync_record(&self, sync: &SyncRecord) -> TResult<()> {
        let noid = String::from("<no id>");
        debug!("SyncOutgoing.delete_sync_record() -- delete {} ({:?} / {:?})", sync.id.as_ref().unwrap_or(&noid), sync.action, sync.ty);
        with_db!{ db, self.db, db.delete(sync)?; }
        Ok(())
    }

    /// Handle each failed sync record, and notify the UI that we have failed
    /// sync items that might need inspection/alerting.
    fn handle_sync_failures(&self, fail: &Vec<SyncRecord>) -> TResult<()> {
        for failure in fail {
            let errmsg = match failure.error.as_ref() {
                Some(err) => err.msg.clone(),
                None => String::from("<blank error>"),
            };
            warn!("SyncOutgoing.handle_sync_failures() -- failwhale: {:?}/{:?}: {}", failure.ty, failure.action, errmsg);
            with_db!{ db, self.db,
                SyncRecord::handle_failed_sync(db, failure)?;
            }
        }
        messaging::ui_event("sync:outgoing:failure", fail)
    }
}

impl Syncer for SyncOutgoing {
    fn get_name(&self) -> &'static str {
        "outgoing"
    }

    fn get_config(&self) -> Arc<RwLock<SyncConfig>> {
        self.config.clone()
    }

    fn get_delay(&self) -> u64 {
        1000
    }

    fn set_run_version(&mut self, run_version: i64) {
        self.run_version = run_version;
    }

    fn get_run_version(&self) -> i64 {
        self.run_version
    }

    fn run_sync(&mut self) -> TResult<()> {
        // get all our sync records queued to be sent out
        let syncs = self.get_outgoing_syncs()?;
        if syncs.len() == 0 { return Ok(()); }

        // send our syncs out to the api, and remove and successful records from
        // our local db
        info!("SyncOutgoing.run_sync() -- sending {} sync items", syncs.len());
        let syncs_json = jedi::to_val(&syncs)?;
        let sync_result: SyncResponse = self.api.post("/sync", ApiReq::new().data(syncs_json))?;
        info!("SyncOutgoing.run_sync() -- got {} successes, {} failed, {} blocked syncs", sync_result.success.len(), sync_result.failures.len(), sync_result.blocked.len());

        // clear out the successful syncs
        let mut err: TResult<()> = Ok(());
        for sync in &sync_result.success {
            // if the record synced successfully, we delete it here
            let res = self.delete_sync_record(sync);
            // grab any extra sync_ids created from this sync item (the api
            // keeps close track of them) and ignore them on the next incoming
            // sync. this keeps us from double-syncing some items.
            let res2 = with_db!{ db, self.db,
                match sync.sync_ids.as_ref() {
                    Some(x) => SyncIncoming::ignore_on_next(db, x),
                    None => Ok(()),
                }
            };
            // track a failure (if it occurs), but then just keep deleting.
            // we don't want to return and have all these sync items re-run
            // just because one of them failed to delete.
            if res.is_err() && err.is_ok() { err = res; }
            if res2.is_err() && err.is_ok() { err = res2; }
        }

        if sync_result.failures.len() > 0 {
            self.handle_sync_failures(&sync_result.failures)?;
        }

        // let the ui know we had an outgoing sync. there are cases where it
        // will want to know this happened.
        messaging::ui_event("sync:outgoing:complete", &())?;

        // if we did indeed get an error while deleting our sync records,
        // send the first error we got back. obviously there may be more
        // than one, but we can only do so much here to maintain resilience.
        err
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::std::sync::{Arc, RwLock, Mutex};
    use ::models::sync_record::SyncRecord;
    use ::jedi;
    use ::schema;

    #[test]
    fn ignores_frozen_syncs() {
        let mut sync_config = SyncConfig::new();
        sync_config.skip_api_init = true;
        let sync_config = Arc::new(RwLock::new(sync_config));
        let api = Arc::new(Api::new());
        let dumpy_schema = schema::get_schema();
        let db = Storage::new(&String::from(":memory:"), dumpy_schema).unwrap();
        let db = Arc::new(Mutex::new(Some(db)));

        let sync1: SyncRecord = jedi::from_val(json!({"id": "1", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        let sync2: SyncRecord = jedi::from_val(json!({"id": "2", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        let mut sync3: SyncRecord = jedi::from_val(json!({"id": "3", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        sync3.frozen = true;

        {
            let mut db_guard = lock!(db);
            let dbo = db_guard.as_mut().unwrap();
            dbo.save(&sync1).unwrap();
            dbo.save(&sync2).unwrap();
            dbo.save(&sync3).unwrap();
        }

        let sync_outgoing = SyncOutgoing::new(sync_config, api, db);
        let outgoing = sync_outgoing.get_outgoing_syncs().unwrap();
        assert_eq!(outgoing.len(), 2);
    }

    #[test]
    fn deserializes_sync_response() {
        let typical_mac_user = String::from(r#"{
            "failures": [],
            "success": [{
                "action": "add",
                "data": {
                    "body": "AAYBAAwz3AotVeFd3dUrlGzQqK7SXNyaW5rOYGKH9kf0gY7Rnv+fa9hPBowlX7vbtF6bB334HXxl5VxaDezZeXwahYhXh7fmVepN6Zt9x9mjsMU+Yg==",
                    "id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0001",
                    "item_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                    "type": "space",
                    "user_id": 190
                },
                "id": "015d7dc40752e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0002",
                "item_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0001",
                "sync_ids": [2165],
                "type": "keychain",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAw4YM40kJQNI6kiweqmj+BmVPvRLATqqx+HVvrBcrIKZe8iq1K3d3Xxx9/CVUj06/4=",
                    "id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                    "members": [{
                        "created": "2017-07-26T07:19:45.701Z",
                        "id": 165,
                        "role": "owner",
                        "space_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                        "updated": "2017-07-26T07:19:45.701Z",
                        "user_id": 190,
                        "username": "slippyslappy@turtlapp.com"
                    }],
                    "user_id": 190
                },
                "id": "015d7dc4077fe58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0003",
                "item_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                "sync_ids": [2166],
                "type": "space",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAyIG44Upjp74zFzuf7yOejqnkWiNaS2B/aWmesUdPYSXdNYyOYqKg6YLvrYAAp+Lhl6AtvcdoEq295MlvFK/hHwqwKCHmkrXlAnrITVqh8qmw==",
                    "id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0005",
                    "item_id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0004",
                    "type": "space",
                    "user_id": 190
                },
                "id": "015d7dc407b8e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0006",
                "item_id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0005",
                "sync_ids": [2167],
                "type": "keychain",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAxSPeOaLXy8vp59v9dmYu565i41c7UKQwmeyw1Z+W5cRtILmhopGJXCgflIhA==",
                    "id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0004",
                    "members": [{
                        "created": "2017-07-26T07:19:45.726Z",
                        "id": 166,
                        "role": "owner",
                        "space_id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0004",
                        "updated": "2017-07-26T07:19:45.726Z",
                        "user_id": 190,
                        "username": "slippyslappy@turtlapp.com"
                    }],
                    "user_id": 190
                },
                "id": "015d7dc407f0e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0007",
                "item_id": "015d7dc40799e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0004",
                "sync_ids": [2168],
                "type": "space",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAw0iUD4BYyFPm4w+KaPCOlpYnN1KimKwUK9LXGf7SKsnwO2dJAMi6w27VlZHzBa/yRCqc6mfIBP/b1B8GI/yXXp8+w4F1A05GddhIckH1nmaw==",
                    "id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0009",
                    "item_id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0008",
                    "type": "space",
                    "user_id": 190
                },
                "id": "015d7dc40825e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000a",
                "item_id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0009",
                "sync_ids": [2169],
                "type": "keychain",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAwl5a6H1PIsrW8HpP49ckO/KZrDjLfGfupTw1N3KfSshWQHzNlMQGWpFHswTg==",
                    "id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0008",
                    "members": [{
                        "created": "2017-07-26T07:19:45.750Z",
                        "id": 167,
                        "role": "owner",
                        "space_id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0008",
                        "updated": "2017-07-26T07:19:45.750Z",
                        "user_id": 190,
                        "username": "slippyslappy@turtlapp.com"
                    }],
                    "user_id": 190
                },
                "id": "015d7dc40855e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000b",
                "item_id": "015d7dc4080de58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0008",
                "sync_ids": [2170],
                "type": "space",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAxffqJ2+3OGYi6W1YoASj0vzLpBf7fVZe32pN4lDQnDTaxIrdossoCtKNGEVgWxvEA/",
                    "id": "015d7dc40870e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000c",
                    "keys": [{
                        "k": "AAYBAAyv1l1jjpalS2A3zVa8hpUmO5jtOt7pEnDnZ3Wifw+XlOl0JUxlMnxA/IWPnlYL4m3oxQXqiHVZSjk7uOQ=",
                        "s": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000"
                    }],
                    "space_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                    "user_id": 190
                },
                "id": "015d7dc40894e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000d",
                "item_id": "015d7dc40870e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000c",
                "sync_ids": [2171],
                "type": "board",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAyWs9FrPq2i4PffSWYt0MlOL/0Fd2YpzMwhsaw5od2V9/yXsOp9slY5nOmm3zHy",
                    "id": "015d7dc408abe58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000e",
                    "keys": [{
                        "k": "AAYBAAy1ZyTE5AsC38jslKTHrfe2EiNu9VKvBxethW3LqB2mNJCBygLGa20ZeTYUluIOKmI1Z0wW5UvTNe5H+WY=",
                        "s": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000"
                    }],
                    "space_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                    "user_id": 190
                },
                "id": "015d7dc408d2e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000f",
                "item_id": "015d7dc408abe58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a000e",
                "sync_ids": [2172],
                "type": "board",
                "user_id": 190
            }, {
                "action": "add",
                "data": {
                    "body": "AAYBAAzVk+6rmeAGseFA2QvbfK5qQ1tpKHTXTq6LN7q1WKUlEO5zTVmwsnORX6inR54T9kyT",
                    "id": "015d7dc408e9e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0010",
                    "keys": [{
                        "k": "AAYBAAyukg85GyoXNKk20e/Rswrk3OpDII/S5INwDXqcSjnubolAdKbXcwBd+WXBhZcMl2sao9Q9gDbtg6bgE6c=",
                        "s": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000"
                    }],
                    "space_id": "015d7dc4073be58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0000",
                    "user_id": 190
                },
                "id": "015d7dc4090ce58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0011",
                "item_id": "015d7dc408e9e58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0010",
                "sync_ids": [2173],
                "type": "board",
                "user_id": 190
            }, {
                "action": "edit",
                "data": {
                    "body": "AAYBAAwdMDWkpn4HU2ZfdPhPnHF/xTwAVspMkj6y0hVzv1jcn2Ku3X2nUl4x60SI1YJDxAZyI+RNa7aTQSZSjtynfvwt8/VEyRH+g9vs+RaWNl3NMwfqjKTD4ZCJbf/WBWQvo0RZU4K1XNIFqJvn4jbOWgvDkqonU4EGWG2QceX+MhItw1/RZMZwEjTtBcFvNn4=",
                    "pubkey": "CkrVJKDpfKLBTVij5b9oEGvaKI/H1SUuOsF2PpYvUyU="
                },
                "id": "015d7dc4092fe58135942d06c3a78eb2350ebf9c4f59b123fc99f726066aa3ede0865114a80a0012",
                "sync_ids": [],
                "type": "user",
                "user_id": 190,
                "item_id": 190
            }]
        }"#);
        let res: SyncResponse = jedi::parse(&typical_mac_user).unwrap();
        assert_eq!(res.success.len(), 10);
        assert_eq!(res.failures.len(), 0);
        assert_eq!(res.blocked.len(), 0);
        let typical_mac_user = String::from(r#"{
            "success": [],
            "failures": [{
                "action": "add",
                "body": null,
                "data": {
                    "body": "AAYBAAwtd8kVjFeP5+9mCNSsLVvprYrjA6EUbcNJxzdJO9SksE80N3NcUQOGiDX+isQo2NM=",
                    "id": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000",
                    "user_id": 363
                },
                "errcount": 1,
                "frozen": false,
                "id": "015dc9bc9adc342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180003",
                "item_id": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000",
                "type": "space",
                "user_id": 363,
                "_id": 1,
                "error": {
                    "code": 500,
                    "msg": "duplicate key value violates unique constraint \"spaces_pkey\""
                }
            }],
            "blocked": [{
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAwkH+SD3EWVhKq7fT8wpI8puWSdV26KppU43havHXqCsHIsTdqBO0iZsgKMBTx9NctkuHG+mDv7zsmTvoiuGnMelyhD7t+FG+V/5sQzso/Qcg==",
                        "id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180005",
                        "item_id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180004",
                        "keys": [],
                        "type": "space",
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180006",
                    "item_id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180005",
                    "type": "keychain",
                    "user_id": 363,
                    "_id": 2
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAw+U8j8awrJGG4pXK6WJk3Xymojk5XEtYG/aAavoRHoUkobFbrvAB+1p00dxg==",
                        "id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180004",
                        "keys": [],
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180007",
                    "item_id": "015dc9bc9add342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180004",
                    "type": "space",
                    "user_id": 363,
                    "_id": 3
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAzTDoznCixb4xQzhXmht5N7oFdqXkibz1hN62dvGC2uLjf4hqEVYmrj0OIrqvmqfHgzTjKV5M2ZvpiyukYdQgJuS5J53ikNluTDU6eVqkBJ3Q==",
                        "id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180009",
                        "item_id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180008",
                        "keys": [],
                        "type": "space",
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000a",
                    "item_id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180009",
                    "type": "keychain",
                    "user_id": 363,
                    "_id": 4
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAzqPb5iKDAo+nDFfWsj+enQCsnV+iJJJTsCl48ykqZhspfkeKkwzFqfEm++eA==",
                        "id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180008",
                        "keys": [],
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9adf342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000b",
                    "item_id": "015dc9bc9ade342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180008",
                    "type": "space",
                    "user_id": 363,
                    "_id": 5
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAwwavtDqqw/NKIAqE+YP1jyAJGRniVLJm3yfl5eaq7rnkPm/pX7DpfEYI5RcNub+hBQ",
                        "id": "015dc9bc9ae0342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000c",
                        "keys": [{
                            "k": "AAYBAAyy0lx0AWjYJZ61DoLUTSTM7QURhOv7G1oH3z+96uI5l0ydV25Uu+IPs8gdzRnISyKrkJsHUyDh5coUzPg=",
                            "s": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000"
                        }],
                        "space_id": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000",
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ae0342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000d",
                    "item_id": "015dc9bc9ae0342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000c",
                    "type": "board",
                    "user_id": 363,
                    "_id": 6
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAyVYC/kqjOEQYiMrt4lxtyc6ysra2LpMz8uR/OukNnhgt6AZuCxxTrGxVKbj0sh",
                        "id": "015dc9bc9ae1342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000e",
                        "keys": [{
                            "k": "AAYBAAy1oUTVoq/uZBfI/nl5DrR1lD/koekp8kNJUpAPb13kKx8nBFUyHx0eQMx1nx3payGHTl6uLwl06vshqaU=",
                            "s": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000"
                        }],
                        "space_id": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000",
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ae1342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000f",
                    "item_id": "015dc9bc9ae1342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a18000e",
                    "type": "board",
                    "user_id": 363,
                    "_id": 7
                },
                {
                    "action": "add",
                    "body": null,
                    "data": {
                        "body": "AAYBAAxMnHpeu7CPTPXUi6kd8Sjha3+LqXfl3W1Ya5PZrDHmfTtk21mY+RFB1SQKRitT+iiJ",
                        "id": "015dc9bc9ae2342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180010",
                        "keys": [{
                            "k": "AAYBAAyEmWq61v/oivmzrVo+IWVod6fJOfYkazjeTvgUz+L9VlfD0c8Lj6wDb6mw9zacAxclzMNxtHwaVdtAl9o=",
                            "s": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000"
                        }],
                        "space_id": "015dc9bc9ada342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180000",
                        "user_id": 363
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ae2342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180011",
                    "item_id": "015dc9bc9ae2342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180010",
                    "type": "board",
                    "user_id": 363,
                    "_id": 8
                },
                {
                    "action": "edit",
                    "body": null,
                    "data": {
                        "body": "AAYBAAzdp+SSxPttlwfVuNLCR+KHpFJ85k1mzsr7LrZLVejOOuf20I8GnQrXIerIh4JnnXybZ4uYHePIP3NtZl36Q41MJs8jkvcd7ItriWPEcjRWY8Gr8QB2svT7ATDizzKQzSqLrh47Yng2IUjNDXKcUHqGt1R7bIJVtbt19DCjBjNcj0Q6b+DUxY5xbcHG+EY=",
                        "id": "363",
                        "keys": [],
                        "pubkey": "NO/1u3IcXViFSOz4EF94uuLfVd8MoFlIZFBX5tkdqmg=",
                        "account_type": 0,
                        "username": "slippyslappy@turtlapp.com"
                    },
                    "errcount": 0,
                    "frozen": false,
                    "id": "015dc9bc9ae3342450f5263739fb6eeb1b93fa049946e0ab1a231cd8c81dd5193de3695b2a180012",
                    "item_id": "363",
                    "type": "user",
                    "user_id": 363,
                    "_id": 9
                }
            ]
        }"#);
        let res: SyncResponse = jedi::parse(&typical_mac_user).unwrap();
        assert_eq!(res.success.len(), 0);
        assert_eq!(res.failures.len(), 1);
        assert_eq!(res.blocked.len(), 8);
    }
}

