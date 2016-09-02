//! The storage module stores things. Don't worry, those things are encrypted.
//! Probably.

use ::std::thread;
use ::std::sync::Arc;
use ::libc::c_int;

use ::crossbeam::sync::MsQueue;
use ::futures::{self, Future, Canceled};
use ::rusqlite::Connection;
use ::rusqlite::types::{ToSql, sqlite3_stmt};

use ::util::opdata::{OpData, OpConverter};
use ::util::thunk::Thunk;
use ::util::thredder::Pipeline;
use ::util::stopper::Stopper;
use ::util::json::{self, JSONError, Value};
use ::models::protected::Protected;
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

/// Start our storage thread, which is the actual keeper of our db Connection.
///
/// We will be talking to is via a channel. It's set up this way because Turtl
/// needs to be shareable across threads but Connection is not Send/Sync so we
/// store an interface to the Connection instead of the Connection itself.
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

/// This enum somewhat matches `json::Value` but we only create the fields we
/// need to map to the ToSql trait.
enum SqVal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String)
}

// Create a conversion from Value -> SqVal
impl From<Value> for SqVal {
    fn from(val: Value) -> SqVal {
        match val {
            Value::Null => SqVal::Null,
            Value::Bool(x) => SqVal::Bool(x),
            Value::I64(x) => SqVal::Int(x),
            Value::F64(x) => SqVal::Float(x),
            Value::U64(x) => SqVal::Int(x as i64),
            Value::String(x) => SqVal::String(x),
            Value::Array(_) | Value::Object(_) => {
                SqVal::String(match json::stringify(&val) {
                    Ok(x) => x,
                    Err(e) => {
                        error!("Value -> ToSql::bind_parameter() -- error stringifying: {}", e);
                        String::from("<error stringifying>")
                    },
                })
            },
        }
    }
}

// ... and implement ToSql for SqVal
impl ToSql for SqVal {
    unsafe fn bind_parameter(&self, stmt: *mut sqlite3_stmt, col: c_int) -> c_int {
        match *self {
            SqVal::Null => (::rusqlite::types::Null).bind_parameter(stmt, col),
            SqVal::Bool(ref x) => x.bind_parameter(stmt, col),
            SqVal::Int(ref x) => x.bind_parameter(stmt, col),
            SqVal::Float(ref x) => x.bind_parameter(stmt, col),
            SqVal::String(ref x) => x.bind_parameter(stmt, col),
        }
    }
}

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

    /// Save a model to our db. Make sure it's serialized before handing it in.
    pub fn save<T>(model: &T) -> TFutureResult<()>
        where T: Protected
    {
        let id = model.id::<String>();
        let model_data = model.untrusted_data();
        let field_names = match model_data {
            Value::Object(ref x) => x.keys(),
            _ => return futures::done(Err(TError::BadValue(format!("Storage::save() -- model data is not an object")))).boxed(),
        };
        let query = match id {
            Some(id) => {
                let mut qry = format!("UPDATE {} SET ", model.table());
                let mut i = 1;
                let mut vals: Vec<SqVal> = Vec::with_capacity(field_names.len() + 1);
                for field in field_names {
                    let null = &Value::Null;
                    let jval: &Value = match json::walk(&[field], &model_data) {
                        Ok(x) => x,
                        Err(JSONError::NotFound(_)) => null,
                        Err(x) => return futures::done(Err(toterr!(x))).boxed(),
                    };
                    // clone the &Value to get a Value (ugh, is there a better
                    // way?)
                    vals.push(From::from(json::to_val(jval)));
                    qry = qry + &format!("{} = ${} ", field, i);
                    i += 1;
                }
                qry = qry + &format!("WHERE id = ${}", i);
                vals.push(SqVal::String(id));
                qry
            },
            None => {
                String::new()
            },
        };
        futures::done(Ok(()))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_queries() {
    }
}
