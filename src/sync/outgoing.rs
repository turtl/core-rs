use ::std::sync::{Arc, RwLock};

use ::jedi;

use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models::model::Model;
use ::models::sync_record::{SyncAction, SyncType, SyncRecord};
use ::models::file_sync::{FileSyncType, FileSync};
use ::turtl::Turtl;
use ::sync::sync_model::SyncModel;

static MAX_ALLOWED_FAILURES: u32 = 3;

#[derive(Deserialize, Debug)]
struct SyncResponse {
    success: Vec<SyncRecord>,
    #[serde(default)]
    failures: Vec<SyncRecord>,
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
    db: Arc<RwLock<Option<Storage>>>,
}

impl SyncOutgoing {
    /// Create a new outgoing syncer
    pub fn new(config: Arc<RwLock<SyncConfig>>, api: Arc<Api>, db: Arc<RwLock<Option<Storage>>>) -> SyncOutgoing {
        SyncOutgoing {
            config: config,
            api: api,
            db: db,
        }
    }

    /// Tells the sync system to unfreeze a sync item so it gets queued to be
    /// included in the next outgoing sync.
    pub fn kick_frozen_sync(turtl: &Turtl, sync_id: &String) -> TResult<()> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::kick_frozen_sync() -- `turtl.db` is empty"))),
        };
        let sync: Option<SyncRecord> = db.get("sync_outgoing", sync_id)?;
        match sync {
            Some(mut rec) => {
                rec.frozen = false;
                db.save(&rec)?;
            }
            None => {}
        }
        Ok(())
    }

    /// Public/static method for deleting a sync record (probably initiated from
    /// the UI).
    pub fn delete_sync_item(turtl: &Turtl, sync_id: &String) -> TResult<()> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::kick_frozen_sync() -- `turtl.db` is empty"))),
        };
        let mut sync_record: SyncRecord = Default::default();
        sync_record.id = Some(sync_id.clone());
        db.delete(&sync_record)?;
        Ok(())
    }

    /// Grab all frozen sync records
    pub fn get_all_frozen(turtl: &Turtl) -> TResult<Vec<SyncRecord>> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::kick_frozen_sync() -- `turtl.db` is empty"))),
        };
        db.find("sync_outgoing", "frozen", &vec![String::from("true")])
    }

    /// Public version of get_outgoing_syncs(). Guess what it does.
    pub fn get_all_pending(turtl: &Turtl) -> TResult<Vec<SyncRecord>> {
        let mut db_guard = turtl.db.write().unwrap();
        let db = match db_guard.as_mut() {
            Some(x) => x,
            None => return Err(TError::MissingField(String::from("SyncOutgoing::kick_frozen_sync() -- `turtl.db` is empty"))),
        };
        db.find("sync_outgoing", "frozen", &vec![String::from("false")])
    }

    /// Grab all outgoing sync items, in order
    fn get_outgoing_syncs(&self) -> TResult<Vec<SyncRecord>> {
        with_db!{ db, self.db, "SyncOutgoing.get_outgoing_syncs()",
            db.find("sync_outgoing", "frozen", &vec![String::from("false")])
        }
    }

    /// Delete a sync record from sync_outgoing (like, when we send it to the
    /// API and it runs successfully...we don't need it sitting around).
    fn delete_sync_record(&self, sync: &SyncRecord) -> TResult<()> {
        let noid = String::from("<no id>");
        debug!("SyncOutgoing.delete_sync_record() -- delete {} ({:?} / {:?})", sync.id.as_ref().unwrap_or(&noid), sync.action, sync.ty);
        with_db!{ db, self.db, "SyncOutgoing.delete_sync_record()", db.delete(sync)?; }
        Ok(())
    }

    /// Increment this SyncRecord's errcount. If it's above a magic number, we
    /// mark the sync as failed, which excludes it from further outgoing syncs
    /// until it gets manually shaken/removed.
    fn handle_failed_sync(&self, failure: &SyncRecord) -> TResult<()> {
        debug!("SyncOutgoing.handle_failed_record() -- handle failure: {:?}", failure);
        let sync_id = match failure.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(format!("SyncOutgoing.handle_failed_record() -- missing `failure.id` field"))),
        };
        let sync_record: Option<SyncRecord> = with_db!{ db, self.db, "SyncOutgoing.handle_failed_record()",
            db.get("sync_outgoing", &sync_id)?
        };
        match sync_record {
            Some(mut rec) => {
                if rec.errcount > MAX_ALLOWED_FAILURES {
                    rec.frozen = true;
                } else {
                    rec.errcount += 1;
                }
                // save our heroic sync record with our mods (errcount/frozen)
                with_db!{ db, self.db, "SyncOutgoing.handle_failed_record()",
                    db.save(&rec)?;
                }
            }
            // already deleted? who knows
            None => {}
        }
        Ok(())
    }

    /// Handle each failed sync record, and notify the UI that we have failed
    /// sync items that might need inspection/alerting.
    fn handle_sync_failures(&self, fail: &Vec<SyncRecord>) -> TResult<()> {
        for failure in fail {
            self.handle_failed_sync(failure)?;
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

    fn run_sync(&mut self) -> TResult<()> {
        let sync = self.get_outgoing_syncs()?;
        if sync.len() == 0 { return Ok(()); }

        // create two collections: one for normal data syncs, and one for files
        let mut syncs: Vec<SyncRecord> = Vec::with_capacity(sync.len());
        let mut file_syncs: Vec<SyncRecord> = Vec::with_capacity(2);
        // split our sync records into our normal/file collections
        for rec in sync {
            if rec.ty == SyncType::File && rec.action == SyncAction::Add {
                file_syncs.push(rec);
            } else {
                syncs.push(rec);
            }
        }

        // send our "normal" syncs out to the api, and remove and successful
        // records from our local db
        if syncs.len() > 0 {
            info!("SyncOutgoing.run_sync() -- sending {} sync items", syncs.len());
            let syncs_json = jedi::to_val(&syncs)?;
            let sync_result: SyncResponse = self.api.post("/sync", ApiReq::new().data(syncs_json))?;
            info!("SyncOutgoing.run_sync() -- got {} successes, {} failed syncs", sync_result.success.len(), sync_result.failures.len());

            // clear out the successful syncs
            let mut err: TResult<()> = Ok(());
            for sync in &sync_result.success {
                let res = self.delete_sync_record(sync);
                // track a failure (if it occurs), but then just keep deleting.
                // we don't want to return and have all these sync items re-run
                // just because one of them failed to delete.
                match res {
                    Ok(_) => (),
                    Err(_) => if err.is_ok() { err = res },
                }
            }

            if sync_result.failures.len() > 0 {
                self.handle_sync_failures(&sync_result.failures)?;
            }

            // if we did indeed get an error while deleting our sync records,
            // send the first error we got back. obviously there may be more
            // than one, but we can only do so much here to maintain resilience.
            err?;
        }

        if file_syncs.len() > 0 {
            for file_sync in file_syncs {
                let mut fsync: FileSync = Default::default();
                let note_id = &file_sync.item_id;
                fsync.id = Some(note_id.clone());
                fsync.ty = FileSyncType::Outgoing;
                with_db!{ db, self.db, "SyncOutgoing.run_sync()",
                    info!("SyncOutgoing.run_sync() -- processing outgoing file sync for note {}", file_sync.item_id);
                    // move the record from sync_outgoing to file_sync
                    fsync.db_save(db)?;
                    file_sync.db_delete(db)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::std::sync::{Arc, RwLock};
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
        let db = Arc::new(RwLock::new(Some(db)));

        let sync1: SyncRecord = jedi::from_val(json!({"id": "1", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        let sync2: SyncRecord = jedi::from_val(json!({"id": "2", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        let mut sync3: SyncRecord = jedi::from_val(json!({"id": "3", "action": "add", "item_id": "69", "user_id": 12, "type": "note"})).unwrap();
        sync3.frozen = true;

        {
            let mut db_guard = db.write().unwrap();
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
    }
}

