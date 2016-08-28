use ::futures::Future;

use ::error::{TResult, TError};
use ::util::{self, json};
use ::util::json::Value;
use ::util::event::Emitter;
use ::turtl::TurtlWrap;
use ::models::user::User;

fn process_res(turtl: TurtlWrap, res: TResult<()>) {
    match res {
        Ok(..) => (),
        Err(e) => match e {
            TError::Shutdown => {
                warn!("dispatch: got shutdown signal, quitting");
                util::sleep(10);
                match turtl.read().unwrap().remote_send("{\"e\":\"shutdown\"}".to_owned()) {
                    Ok(..) => (),
                    Err(..) => (),
                }
                util::sleep(10);
                let ref mut events = turtl.write().unwrap().events;
                events.trigger("app:shutdown", &json::to_val(&()));
            }
            _ => error!("dispatch: error processing message: {}", e),
        },
    };
}

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = try!(json::parse(msg));

    // grab the command from the data
    let cmd: String = try!(json::get(&["0"], &data));

    let res = match cmd.as_ref() {
        "user:login" => {
            let username = try!(json::get(&["1", "username"], &data));
            let password = try!(json::get(&["1", "password"], &data));
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            User::login(turtl.clone(), &username, &password)
                .map(move |_| {
                    let turtl_inner = turtl1.read().unwrap();
                    match turtl_inner.remote_send(String::from(r#"{"e":"login"}"#)) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .map_err(move |_| {
                    let mut turtl_inner = turtl2.write().unwrap();
                    turtl_inner.api.clear_auth();
                    match turtl_inner.remote_send(String::from(r#"{"e":"error","data":{"name":"login-failed"}}"#)) {
                        Err(e) => error!("dispatch -- problem sending login message: {}", e),
                        _ => ()
                    }
                })
                .forget();
            Ok(())
        },
        "ping" => {
            info!("ping!");
            turtl.read().unwrap().remote_send("{\"e\":\"pong\"}".to_owned())
                .map(|_| ())
        }
        "shutdown" => Err(TError::Shutdown),
        _ => Err(TError::Msg(format!("bad command: {}", cmd))),
    };
    process_res(turtl, res);
    Ok(())
}

