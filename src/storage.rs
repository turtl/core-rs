//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::thread;
use ::std::sync::Arc;

use ::crossbeam::sync::MsQueue;
use ::rusqlite::Connection;
use ::harrrrsqlite::Adapter;
use ::futures::{self, Future, Canceled};

use ::util::opdata::{OpData, OpConverter};
use ::util::thunk::Thunk;
use ::util::thredder::Pipeline;
use ::util::stopper::Stopper;
use ::turtl::TurtlWrap;

use ::error::{TResult, TFutureResult, TError};

pub type StorageMsg = Box<Thunk<Arc<Connection>>>;
pub type StorageSender = Arc<MsQueue<StorageMsg>>;

lazy_static!{
    static ref RUN: Stopper = Stopper::new();
}

/// Stop the storage thread loop
pub fn stop(storage: &Storage) {
    (*RUN).set(false);
    storage.run(|_| -> TResult<()> {
        Ok(())
    });
}

fn start(location: &String) -> StorageSender {
    (*RUN).set(true);
    let queue = Arc::new(MsQueue::new());
    let recv = queue.clone();
    thread::spawn(move || {
        let conn = match Connection::open_in_memory() {
            Ok(x) => Arc::new(x),
            Err(e) => {
                error!("storage::start() -- {}", e);
                return;
            }
        };
        while (*RUN).running() {
            let handler: StorageMsg = recv.pop();
            handler.call_box(conn.clone());
        }
        info!("storage::start() -- shutting down");
    });
    queue
}

/// This structure holds state for persisting (encrypted) data to disk.
pub struct Storage {
    tx: StorageSender,
    tx_main: Pipeline,
}

unsafe impl Send for Storage {}

impl Storage {
    /// Make a Storage lol
    pub fn new(tx_main: Pipeline, location: &String) -> TResult<Storage> {
        Ok(Storage {
            tx: start(location),
            tx_main: tx_main,
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
}

