//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::thread;
use ::std::sync::Arc;
use ::std::ops::Drop;

use ::crossbeam::sync::MsQueue;
use ::futures::{self, Future, Canceled};
use ::rusqlite::Connection;

use ::util::opdata::{OpData, OpConverter};
use ::util::thunk::Thunk;
use ::util::thredder::Pipeline;
use ::util::stopper::Stopper;
use ::models::model::ModelDataRef;
use ::models::protected::Protected;
use ::turtl::TurtlWrap;

use ::error::{TResult, TFutureResult, TError};

pub type StorageMsg = Box<Thunk<Arc<Connection>>>;
pub type StorageSender = Arc<MsQueue<StorageMsg>>;

/// Start our storage thread, which is the actual keeper of our db Connection.
///
/// We will be talking to it via a channel. It's set up this way because Turtl
/// needs to be shareable across threads but Connection is not Send/Sync so we
/// store an interface to the Connection instead of the Connection itself.
fn start(location: &String) -> (StorageSender, thread::JoinHandle<()>, Arc<Stopper>) {
    let stopper = Arc::new(Stopper::new());
    stopper.set(true);
    let queue = Arc::new(MsQueue::new());
    let recv = queue.clone();
    let location = String::from(&location[..]);
    let stopper_local = stopper.clone();
    let handle = thread::spawn(move || {
        let conn;
        if location == ":inmem:" {
            conn = Connection::open_in_memory();
        } else {
            conn = Connection::open_in_memory();
        }
        let conn = match conn {
            Ok(x) => Arc::new(x),
            Err(e) => {
                error!("storage::start() -- {}", e);
                return;
            }
        };
        while stopper_local.running() {
            let handler: StorageMsg = recv.pop();
            handler.call_box(conn.clone());
        }
        info!("storage::start() -- shutting down");
    });
    (queue, handle, stopper)
}

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    tx: StorageSender,
    tx_main: Pipeline,
    pub handle: thread::JoinHandle<()>,
    stopper: Arc<Stopper>,
}

impl Storage {
    /// Make a Storage lol
    pub fn new(tx_main: Pipeline, location: &String) -> TResult<Storage> {
        let (tx, handle, stopper) = start(location);
        Ok(Storage {
            tx: tx,
            tx_main: tx_main,
            handle: handle,
            stopper: stopper,
        })
    }

    /// Run a query
    pub fn run<F, T>(&self, run: F) -> TFutureResult<T>
        where T: OpConverter + Send + 'static,
              F: FnOnce(Arc<Connection>) -> TResult<T> + Sync + Send + 'static
    {
        let (fut_tx, fut_rx) = futures::oneshot::<TResult<OpData>>();
        let tx_main = self.tx_main.clone();
        self.tx.push(Box::new(move |conn: Arc<Connection>| {
            let res: TResult<OpData> = run(conn).map(|x| x.to_opdata());
            tx_main.push(Box::new(move |_: TurtlWrap| { fut_tx.complete(res) }));
        }));
        fut_rx
            .then(move |res: Result<TResult<OpData>, Canceled>| {
                match res {
                    Ok(x) => match x {
                        Ok(x) => futures::done(OpData::to_value(x)),
                        Err(x) => futures::done(Err(x)),
                    },
                    Err(_) => futures::done(Err(TError::Msg(format!("storage: oneshot future canceled")))),
                }
            })
            .boxed()
    }

    /// Stop the storage thread
    pub fn stop(&self) {
        self.stopper.set(false);
        self.run(|_| -> TResult<()> {
            Ok(())
        });
    }

    /// Save a model to our db. Make sure it's serialized before handing it in.
    pub fn save<T>(model: &T) -> TFutureResult<()>
        where T: Protected
    {
        let id = model.id::<String>();
        let fields = model.public_fields();
        let query = match id {
            Some(_) => {
                let mut qry = format!("UPDATE {} SET ", model.table());
                let mut i = 1;
                let mut vals: Vec<ModelDataRef> = Vec::with_capacity(fields.len() + 1);
                for field in fields {
                    vals.push(model.get_raw(field));
                    qry = qry + &format!("{} = ${} ", field, i);
                    i += 1;
                }
                qry = qry + &format!("WHERE id = ${}", i);
                vals.push(ModelDataRef::String(id));
                qry
            },
            None => {
                String::new()
            },
        };
        println!("query: {}", query);
        futures::done(Ok(()))
            .boxed()
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::std::sync::{Arc, RwLock};

    use ::crossbeam::sync::MsQueue;
    use ::futures::Future;

    use ::error::TResult;
    use ::turtl::{Turtl, TurtlWrap};
    use ::util::stopper::Stopper;

    fn fake_turtl() -> TurtlWrap {
        Turtl::new_wrap(
            Arc::new(MsQueue::new()),
            Arc::new(MsQueue::new()),
            &String::from(":inmem:")
        ).unwrap()
    }

    #[test]
    fn runs_queries() {
        let stopper = Arc::new(Stopper::new());
        stopper.set(true);
        let tx_main = Arc::new(MsQueue::new());
        let db = Arc::new(Storage::new(tx_main.clone(), &String::from(":inmem:")).unwrap());
        let stopclone = stopper.clone();
        let dbclone = db.clone();
        let turtl = fake_turtl();

        let id = Arc::new(RwLock::new(0u64));
        let name = Arc::new(RwLock::new(String::new()));
        let err: Arc<RwLock<TResult<()>>> = Arc::new(RwLock::new(Ok(())));
        let idclone = id.clone();
        let nameclone = name.clone();
        let errclone = err.clone();
        db.run(move |conn| -> TResult<_> {
            try!(conn.execute("CREATE TABLE shibas (id integer primary key, name varchar(255))", &[]));
            try!(conn.execute("INSERT INTO shibas (name) VALUES ($1)", &[&String::from("Kofi")]));
            let mut res = try!(conn.prepare("SELECT id, name FROM shibas LIMIT 1"));
            let rows = try!(res.query_map(&[], |row| {
                let id: i64 = row.get(0);
                let name: String = row.get(1);
                (id, name)
            }));
            for row in rows {
                let (id, name) = row.unwrap();
                *(idclone.write().unwrap()) = id as u64;
                *(nameclone.write().unwrap()) = name;
            }
            Ok(())
        }).and_then(|_| {
            ::futures::finished(())
        }).or_else(move |e| {
            *(errclone.write().unwrap()) = Err(e);
            ::futures::finished::<(), ()>(())
        }).then(move |_| {
            stopclone.set(false);
            dbclone.stop();
            ::futures::finished::<(), ()>(())
        }).forget();

        while stopper.running() {
            //debug!("turtl: main thread message loop");
            let handler = tx_main.pop();
            handler.call_box(turtl.clone());
        }

        assert!((*(id.read().unwrap())) > 0);
        assert_eq!(*(name.read().unwrap()), "Kofi");
        assert!((*(err.read().unwrap())).is_ok());
    }
}
