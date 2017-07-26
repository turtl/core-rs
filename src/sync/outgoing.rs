use ::std::sync::{Arc, RwLock};

use ::jedi;

use ::error::TResult;
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
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
        debug!("SyncOutgoing.delete_sync_record() -- delete {} ({:?} {})", sync.id.as_ref().unwrap_or(&noid), sync.action, sync.ty);
        with_db!{ db, self.db, "SyncOutgoing.delete_sync_record()",
            db.conn.execute("DELETE FROM sync_outgoing WHERE id = $1", &[&sync.id])?;
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
            "success": [{
                "id": "015d7db2c0d12ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70002",
                "user_id": 187,
                "item_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70001",
                "type": "keychain",
                "action": "add",
                "sync_ids": [2129],
                "data": {
                    "id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70001",
                    "body": "AAYBAAz3+hdLuX3OoNYIXdTYJOp9keFeNGlvRFL4MiXsBgaa3J3sZ1SZmliayh8HDf4/RaabH6zSWSBK9+i1/NlY8bNyUr2kPhbi9mNX6ufB81w9tA==",
                    "type": "space",
                    "item_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000",
                    "user_id": 187
                }
            }, {
                "id": "015d7db2c1012ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70003",
                "user_id": 187,
                "item_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000",
                "type": "space",
                "action": "add",
                "sync_ids": [2130],
                "data": {
                    "id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000",
                    "body": "AAYBAAxpk7LxYWqblPdAqb7S5umXqNC+DCCHmDHkX8OmMgxzfNcktnjPprW+a47l4wmkrOw=",
                    "user_id": 187,
                    "members": [{
                        "id": 156,
                        "space_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000",
                        "user_id": 187,
                        "role": "owner",
                        "created": "2017-07-26T07:00:53.516Z",
                        "updated": "2017-07-26T07:00:53.516Z",
                        "username": "slippyslappy@turtlapp.com"
                    }]
                }
            }, {
                "id": "015d7db2c12f2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70006",
                "user_id": 187,
                "item_id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70005",
                "type": "keychain",
                "action": "add",
                "sync_ids": [2131],
                "data": {
                    "id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70005",
                    "body": "AAYBAAxJBaGKiNGD75OrycfpRoc0csnrXVgXhR1yATkYOQVVSmiZ7QMTfNovJQC4bPai5iDlhM2wiTXVvzauPg94CmP4H5Ff/z+gmPjBIM9i9E3cqA==",
                    "type": "space",
                    "item_id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70004",
                    "user_id": 187
                }
            }, {
                "id": "015d7db2c1642ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70007",
                "user_id": 187,
                "item_id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70004",
                "type": "space",
                "action": "add",
                "sync_ids": [2132],
                "data": {
                    "id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70004",
                    "body": "AAYBAAzqQbPuVE91ThF/o7w/fXaguodk1YgIWvL5veo7yVMfJwOIlMJv6pXt5NUytA==",
                    "user_id": 187,
                    "members": [{
                        "id": 157,
                        "space_id": "015d7db2c1182ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70004",
                        "user_id": 187,
                        "role": "owner",
                        "created": "2017-07-26T07:00:53.561Z",
                        "updated": "2017-07-26T07:00:53.561Z",
                        "username": "slippyslappy@turtlapp.com"
                    }]
                }
            }, {
                "id": "015d7db2c1902ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000a",
                "user_id": 187,
                "item_id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70009",
                "type": "keychain",
                "action": "add",
                "sync_ids": [2133],
                "data": {
                    "id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70009",
                    "body": "AAYBAAwjyacMLB35fvlkN8H3IVm41RBh5GsufEm+/iKeGfhjZ3W5GsDhAf46GQjRhpVmOEX7WO1za0ZU3yXZ4ID8fRBvhsGlwFQlLloyYOQ0ngSsNw==",
                    "type": "space",
                    "item_id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70008",
                    "user_id": 187
                }
            }, {
                "id": "015d7db2c1be2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000b",
                "user_id": 187,
                "item_id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70008",
                "type": "space",
                "action": "add",
                "sync_ids": [2134],
                "data": {
                    "id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70008",
                    "body": "AAYBAAw0ca29N5KGorFiB2XqdpzkoFtg+2dRifY3YzxbA+wRzmHlCF8eJpmVc37FcA==",
                    "user_id": 187,
                    "members": [{
                        "id": 158,
                        "space_id": "015d7db2c1792ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70008",
                        "user_id": 187,
                        "role": "owner",
                        "created": "2017-07-26T07:00:53.588Z",
                        "updated": "2017-07-26T07:00:53.588Z",
                        "username": "slippyslappy@turtlapp.com"
                    }]
                }
            }, {
                "id": "015d7db2c1f72ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000d",
                "user_id": 187,
                "item_id": "015d7db2c1d52ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000c",
                "type": "board",
                "action": "add",
                "sync_ids": [2135],
                "data": {
                    "id": "015d7db2c1d52ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000c",
                    "body": "AAYBAAwVidQuy3QEohto4tkqbhHw9GpK9P6uBIVQgPJLVEqK7pQ/SXa7PfVYS2vRg/Uy7t7C",
                    "keys": [{
                        "k": "AAYBAAxaE0d2iHJ6wgTt1KGYKvHA+c8fnp6bTqnmf0j8Bwjdd4jQCW+7uv9RvEWoZPRAwCZi8K3fqXD8KEpcvhw=",
                        "s": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                    }],
                    "user_id": 187,
                    "space_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                }
            }, {
                "id": "015d7db2c22e2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000f",
                "user_id": 187,
                "item_id": "015d7db2c20d2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000e",
                "type": "board",
                "action": "add",
                "sync_ids": [2136],
                "data": {
                    "id": "015d7db2c20d2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b7000e",
                    "body": "AAYBAAzQzxlBn5btIdu1W+ghvg1dGcIBb5KisMfbIlXuA0wUepveMkP6kl9Gmycb3O6k",
                    "keys": [{
                        "k": "AAYBAAx0XFs6HOEM/m3MPlLX07sUczY9njRXOVaEZcu+QJFVEq9VjT1cfXKdPKXoG3zfAs+AMCflxBTtcycvofo=",
                        "s": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                    }],
                    "user_id": 187,
                    "space_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                }
            }, {
                "id": "015d7db2c2692ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70011",
                "user_id": 187,
                "item_id": "015d7db2c2442ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70010",
                "type": "board",
                "action": "add",
                "sync_ids": [2137],
                "data": {
                    "id": "015d7db2c2442ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70010",
                    "body": "AAYBAAzZsJaXHd98bauZcj0m2S8qerUdM6ANclqW01m7h/nRXoepw7gAGyc2l8DfNHQdaE0X",
                    "keys": [{
                        "k": "AAYBAAwwkJ8qFNdK3gkWOIifDFOmCuDl9rvRSm/BDMDoYBQrjBJQrUhJCXz3KxcFUZgy64djv/HWZGcNztT//OU=",
                        "s": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                    }],
                    "user_id": 187,
                    "space_id": "015d7db2c0b92ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70000"
                }
            }, {
                "id": "015d7db2c28a2ec83616a8c69cb3701422594ad3a2d143ecd36867ad00ce078c8a54d88254b70012",
                "user_id": 187,
                "type": "user",
                "action": "edit",
                "sync_ids": [],
                "data": {
                    "body": "AAYBAAxL3NdwlBG5MS1M2A+rWAx+prp7rVndT/KJT9jWsQ5iBZ8D8/hUSmMhiCUxGTrckD5eSxQOiU+PwhhmG8+cu7xKzC7Vrql6pD2RKCAcNg5qY13smLFpZEPAmuQ6te94RNNwsAXj6HwEhOgBnaen6DicXnkyCAK4+ZttwsUm4AUl8jNBJgYQ2Oz7snrsW+A=",
                    "pubkey": "q2b9mKWhEmlB1FNGqYbiGIDI521HUop8NEL9xL+87So="
                }
            }],
            "failures": []
        }"#);
        let res: SyncResponse = jedi::parse(&typical_mac_user).unwrap();
        assert_eq!(res.success.len(), 10);
    }
}

