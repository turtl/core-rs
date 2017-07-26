use ::std::sync::{Arc, RwLock};

use ::jedi;

use ::error::{TResult, TError};
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models::model::Model;
use ::models::sync_record::{SyncAction, SyncRecord};

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

    /// Grab all outgoing sync items, in order
    fn get_outgoing_syncs(&self) -> TResult<Vec<SyncRecord>> {
        let outgoing = with_db!{ db, self.db, "SyncOutgoing.get_outgoing_syncs()",
            db.all("sync_outgoing")?
        };
        let mut objects: Vec<SyncRecord> = Vec::new();
        for data in outgoing {
            objects.push(jedi::from_val(data)?);
        }
        Ok(objects)
    }

    /// Delete a sync record
    fn delete_sync_record(&self, sync: &SyncRecord) -> TResult<()> {
        let noid = String::from("<no id>");
        debug!("SyncOutgoing.delete_sync_record() -- delete {} ({:?} / {})", sync.id.as_ref().unwrap_or(&noid), sync.action, sync.ty);
        let sync_id = match sync.id().as_ref() {
            Some(id) => id.clone(),
            None => return Err(TError::MissingField(String::from("SyncOutgoing.delete_sync_record() -- sync record missing `id` field"))),
        };
        with_db!{ db, self.db, "SyncOutgoing.delete_sync_record()",
            db.dumpy.delete(&db.conn, &String::from("sync_outgoing"), &sync_id)?;
        }
        Ok(())
    }

    // TODO: mark the sync item as failed
    fn freeze_sync_record(&self, _sync: &SyncRecord) -> TResult<()> {
        Ok(())
    }

    /// Get how many times a sync record has failed
    fn get_errcount(&self, sync: &SyncRecord) -> TResult<u32> {
        let query = "SELECT errcount FROM sync_outgoing WHERE id = $1 LIMIT 1";
        let mut errcount: u32 = 0;
        with_db!{ db, self.db, "SyncOutgoing.get_errcount()",
            let mut query = db.conn.prepare(query)?;
            let rows = query.query_map(&[&sync.id], |row| {
                let count: i64 = row.get("errcount");
                count
            })?;
            for data in rows {
                match data {
                    Ok(x) => errcount = x as u32,
                    Err(_) => (),
                }
                break;
            }
        };
        Ok(errcount)
    }

    /// Set errcount += 1 to the given sync record
    fn increment_errcount(&self, sync: &SyncRecord) -> TResult<()> {
        with_db!{ db, self.db, "SyncOutgoing.get_errcount()",
            db.conn.execute("UPDATE sync_outgoing SET errcount = errcount + 1 WHERE id = $1", &[&sync.id])?;
        }
        Ok(())
    }

    /// Increment this SyncRecord's errcount. If it's above a magic number, we
    /// delete the record.
    fn handle_failed_record(&self, failure: &SyncRecord) -> TResult<()> {
        debug!("SyncOutgoing.handle_failed_record() -- handle failure: {:?}", failure);
        let errcount = self.get_errcount(failure)?;
        if errcount > MAX_ALLOWED_FAILURES {
            self.freeze_sync_record(failure)
        } else {
            self.increment_errcount(failure)
        }
    }

    /// Notify the app that we have sync failure(s), and also update the error
    /// count on those records.
    /// TODO: implement embedded errors
    fn notify_sync_failure(&self, fail: &Vec<SyncRecord>) -> TResult<()> {
        for failure in fail {
            self.handle_failed_record(failure)?;
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

    fn init(&self) -> TResult<()> {
        Ok(())
    }

    fn run_sync(&self) -> TResult<()> {
        let sync = self.get_outgoing_syncs()?;
        if sync.len() == 0 { return Ok(()); }

        // create two collections: one for normal data syncs, and one for files
        let mut syncs: Vec<SyncRecord> = Vec::new();
        let mut file_syncs: Vec<SyncRecord> = Vec::new();
        // split our sync records into our normal/file collections
        for rec in sync {
            if rec.ty == "file" && rec.action == SyncAction::Add {
                file_syncs.push(rec);
            } else {
                syncs.push(rec);
            }
        }

        // send our "normal" syncs out to the api, and remove and successful
        // records from our local db
        if syncs.len() > 0 {
            info!("SyncOutgoing.run_sync() -- sending {} sync items", syncs.len());
            let sync_result: SyncResponse = self.api.post("/sync", ApiReq::new().data(jedi::to_val(&syncs)?))?;
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
                self.notify_sync_failure(&sync_result.failures)?;
            }

            // if we did indeed get an error while deleting our sync records,
            // send the first error we got back. obviously there may be more
            // than one, but we can only do so much here to maintain resilience.
            err?;
        }

        if file_syncs.len() > 0 {
            // TODO: queue file outgoing sync and remove sync_outgoing recs
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

