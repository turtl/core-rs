//! Dispatch takes messages sent from our wonderful UI and runs the needed core
//! code to generate the response. Essentially, it's the RPC endpoint for core.
//!
//! Each message sent in is in the following format (JSON):
//! 
//!     ["<message id>", "<command>", arg1, arg2, ...]
//!
//! where the arg\* can be any valid JSON object. The Message ID is passed in
//! when responding so the client knows which request we are responding to.

use ::futures::{self, Future};
use ::jedi::{self, Value};
use ::config;

use ::error::{TResult, TError, TFutureResult};
use ::util;
use ::util::event::Emitter;
use ::turtl::TurtlWrap;
use ::search::Query;
use ::models::note::Note;
use ::models::protected;

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = jedi::parse(msg)?;

    // grab the request id from the data
    let mid: String = match jedi::get(&["0"], &data) {
        Ok(x) => x,
        Err(_) => return Err(TError::MissingField(String::from("missing mid (0)"))),
    };
    // grab the command from the data
    let cmd: String = match jedi::get(&["1"], &data) {
        Ok(x) => x,
        Err(_) => return Err(TError::MissingField(String::from("missing cmd (1)"))),
    };

    info!("dispatch({}): {}", mid, cmd);
    match cmd.as_ref() {
        "user:login" => {
            let username = jedi::get(&["2", "username"], &data)?;
            let password = jedi::get(&["2", "password"], &data)?;
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            let mid = mid.clone();
            let mid2 = mid.clone();
            turtl.login(username, password)
                .map(move |_| {
                    debug!("dispatch({}) -- user:login success", mid);
                    match turtl1.msg_success(&mid, jedi::obj()) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .map_err(move |e| {
                    turtl2.api.clear_auth();
                    match turtl2.msg_error(&mid2, &e) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .forget();
            Ok(())
        },
        "user:logout" => {
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            let mid1 = mid.clone();
            let mid2 = mid.clone();
            turtl.logout()
                .then(|res| {
                    util::sleep(1000);
                    futures::done(res)
                })
                .map(move |_| {
                    debug!("dispatch({}) -- user:login success", mid);
                    match turtl1.msg_success(&mid1, jedi::obj()) {
                        Err(e) => error!("dispatch -- problem sending logout message: {}", e),
                        _ => ()
                    }
                })
                .map_err(move |e| {
                    turtl2.api.clear_auth();
                    match turtl2.msg_error(&mid2, &e) {
                        Err(e) => error!("dispatch -- problem sending logout message: {}", e),
                        _ => ()
                    }
                })
                .forget();
            Ok(())
        },
        "user:join" => {
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:start-sync" => {
            turtl.start_sync()?;
            let turtl2 = turtl.clone();
            turtl.events.bind_once("sync:incoming:init:done", move |err| {
                // using our crude eventing system, a bool signals a success, a
                // string is an error (containing the error message)
                match *err {
                    Value::Bool(_) => {
                        try_or!(turtl2.msg_success(&mid, jedi::obj()), e,
                            error!("dispatch -- app:start-sync: error sending success: {}", e));
                    },
                    Value::String(ref x) => {
                        try_or!(turtl2.msg_error(&mid, &TError::Msg(x.clone())), e,
                            error!("dispatch -- app:start-sync: error sending error: {}", e));
                    },
                    _ => {
                        error!("dispatch -- unknown sync error: {:?}", err);
                        try_or!(turtl2.msg_error(&mid, &TError::Msg(String::from("unknown error initializing syncing"))), e,
                            error!("dispatch -- app:start-sync: error sending error: {}", e));
                    },
                }
            }, "dispatch:sync:init");
            Ok(())
        },
        "app:pause-sync" => {
            turtl.events.trigger("sync:pause", &jedi::obj());
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:resume-sync" => {
            turtl.events.trigger("sync:resume", &jedi::obj());
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:shutdown-sync" => {
            turtl.events.trigger("sync:shutdown", &Value::Bool(true));
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:api:set-endpoint" => {
            let endpoint: String = jedi::get(&["2"], &data)?;
            config::set(&["api", "endpoint"], &endpoint)?;
            turtl.msg_success(&mid, jedi::obj())
        },
        "app:shutdown" => {
            info!("dispatch: got shutdown signal, quitting");
            match turtl.msg_success(&mid, jedi::obj()) {
                Ok(..) => (),
                Err(..) => (),
            }
            util::sleep(10);
            turtl.events.trigger("app:shutdown", &jedi::to_val(&()));
            Ok(())
        },
        "profile:get-notes" => {
            let qry: Query = jedi::get(&["2", "search"], &data)?;
            let search_guard = turtl.search.read().unwrap();
            if search_guard.is_none() {
                return Err(TError::MissingField(String::from("dispatch: profile:get-notes -- turtl is missing `search` object")));
            }
            let db_guard = turtl.db.read().unwrap();
            if db_guard.is_none() {
                return Err(TError::MissingField(String::from("dispatch: profile:get-notes -- turtl is missing `db` object")));
            }
            let search = search_guard.as_ref().unwrap();
            let db = db_guard.as_ref().unwrap();
            let note_ids = search.find(&qry)?;
            let notes: Vec<Note> = jedi::from_val(Value::Array(db.by_id("notes", &note_ids)?))?;
            let mid1 = mid.clone();
            let mid2 = mid.clone();
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            protected::map_deserialize(turtl.clone().as_ref(), notes)
                .and_then(move |notes: Vec<Note>| -> TFutureResult<()> {
                    FOk!(ftry!(turtl1.msg_success(&mid1, jedi::to_val(&notes))))
                })
                .or_else(move |e| -> TFutureResult<()> {
                    match turtl2.msg_error(&mid2, &e) {
                        Err(e) => error!("dispatch -- problem sending get-notes message: {}", e),
                        _ => ()
                    }
                    FOk!(())
                })
                .forget();
            Ok(())
        },
        "ping" => {
            info!("ping!");
            turtl.msg_success(&mid, Value::String(String::from("pong")))
        },
        _ => {
            match turtl.msg_error(&mid, &TError::MissingCommand(cmd.clone())) {
                Err(e) => error!("dispatch -- problem sending error message: {}", e),
                _ => ()
            }
            Err(TError::Msg(format!("bad command: {}", cmd)))
        }
    }
}

