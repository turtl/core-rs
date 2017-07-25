use ::std::sync::{Arc, RwLock};

use ::jedi::{self, Value};

use ::error::TResult;
use ::sync::{SyncConfig, Syncer};
use ::storage::Storage;
use ::api::{Api, ApiReq};
use ::messaging;
use ::models::sync_record::SyncRecord;

static MAX_ALLOWED_FAILURES: u32 = 3;

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
            db.all("outgoing_sync")?
        };
        let mut objects: Vec<SyncRecord> = Vec::new();
        for data in outgoing {
            objects.push(jedi::from_val(data)?);
        }
        Ok(objects)
    }

    /// Delete a sync record
    fn delete_sync_record(&self, sync: &SyncRecord) -> TResult<()> {
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
    fn notify_sync_failure(&self, fail: Vec<SyncRecord>, error: Value) -> TResult<()> {
        for failure in &fail {
            self.handle_failed_record(failure)?;
        }
        let fail_val = jedi::to_val(&fail)?;
        messaging::ui_event("sync:outgoing:failure", &Value::Array(vec![fail_val, error]))
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
            if rec.ty == "file" && rec.action == "add" {
                file_syncs.push(rec);
            } else {
                syncs.push(rec);
            }
        }

        // send our "normal" syncs out to the api, and remove and successful
        // records from our local db
        if syncs.len() > 0 {
            let sync_result = self.api.post("/sync", ApiReq::new().data(jedi::to_val(&syncs)?))?;

            // our successful syncs
            let success: Vec<SyncRecord> = jedi::get(&["success"], &sync_result)?;
            // our failed syncs
            let fails: Vec<SyncRecord> = jedi::get(&["fail"], &sync_result)?;
            // the error (if any) we got while syncing
            let error: Value = jedi::get(&["error"], &sync_result)?;

            // clear out the successful syncs
            let mut err: TResult<()> = Ok(());
            for sync in &success {
                let res = self.delete_sync_record(sync);
                // track a failure (if it occurs), but then just keep deleting.
                // we don't want to return and have all these sync items re-run
                // just because one of them failed to delete.
                match res {
                    Ok(_) => (),
                    Err(_) => if err.is_ok() { err = res },
                }
            }

            if fails.len() > 0 || error != Value::Null {
                self.notify_sync_failure(fails, error)?;
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

