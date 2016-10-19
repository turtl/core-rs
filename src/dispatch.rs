//! Dispatch takes messages sent from our wonderful UI and runs the needed core
//! code to generate the response. Essentially, it's the RPC endpoint for core.
//!
//! Each message sent in is in the following format (JSON):
//! 
//!     ["<message id>", "<command>", arg1, arg2, ...]
//!
//! where the arg\* can be any valid JSON object. The Message ID is passed in
//! when responding so the client knows which request we are responding to.

use ::futures::Future;
use ::jedi::{self, Value};
use ::config;

use ::error::{TResult, TError};
use ::util;
use ::util::event::Emitter;
use ::turtl::TurtlWrap;
use ::models::user::User;

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = try!(jedi::parse(msg));

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

    match cmd.as_ref() {
        "user:login" => {
            let username = try!(jedi::get(&["2", "username"], &data));
            let password = try!(jedi::get(&["2", "password"], &data));
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            let mid = mid.clone();
            let mid2 = mid.clone();
            User::login(turtl.clone(), &username, &password)
                .map(move |_| {
                    let turtl_inner = turtl1.read().unwrap();
                    match turtl_inner.msg_success(&mid, jedi::obj()) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                    // TODO: init turtl.db w/ dumpy schema:
                    //   let dumpy_schema = try!(config::get::<Value>(&["schema"]));
                    // TODO: start sync system
                })
                .map_err(move |e| {
                    let mut turtl_inner = turtl2.write().unwrap();
                    turtl_inner.api.write().unwrap().clear_auth();
                    match turtl_inner.msg_error(&mid2, &e) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .forget();
            Ok(())
        },
        "app:api:set_endpoint" => {
            let endpoint: String = try!(jedi::get(&["2"], &data));
            try!(config::set(&["api", "endpoint"], &endpoint));
            turtl.read().unwrap().msg_success(&mid, jedi::obj())
        },
        "app:shutdown" => {
            info!("dispatch: got shutdown signal, quitting");
            match turtl.read().unwrap().msg_success(&mid, jedi::obj()) {
                Ok(..) => (),
                Err(..) => (),
            }
            util::sleep(10);
            let ref mut events = turtl.write().unwrap().events;
            events.trigger("app:shutdown", &jedi::to_val(&()));
            Ok(())
        },
        "ping" => {
            info!("ping!");
            turtl.read().unwrap().msg_success(&mid, Value::String(String::from("pong")))
                .map(|_| ())
        },
        _ => {
            match turtl.read().unwrap().msg_error(&mid, &TError::MissingCommand(cmd.clone())) {
                Err(e) => error!("dispatch -- problem sending error message: {}", e),
                _ => ()
            }
            Err(TError::Msg(format!("bad command: {}", cmd)))
        }
    }
}

