use ::futures::Future;
use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::util;
use ::util::event::Emitter;
use ::turtl::TurtlWrap;
use ::models::user::User;

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = try!(jedi::parse(msg));

    // grab the command from the data
    let cmd: String = try!(jedi::get(&["0"], &data));

    let res = match cmd.as_ref() {
        "user:login" => {
            let username = try!(jedi::get(&["1", "username"], &data));
            let password = try!(jedi::get(&["1", "password"], &data));
            let turtl1 = turtl.clone();
            let turtl2 = turtl.clone();
            User::login(turtl.clone(), &username, &password)
                .map(move |_| {
                    let turtl_inner = turtl1.read().unwrap();
                    match turtl_inner.remote_send(String::from(r#"{"e":"login-success"}"#)) {
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
        "app:shutdown" => {
            info!("dispatch: got shutdown signal, quitting");
            match turtl.read().unwrap().remote_send("{\"e\":\"shutdown\"}".to_owned()) {
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
            turtl.read().unwrap().remote_send(String::from(r#"{"e":"pong"}"#))
                .map(|_| ())
        },
        _ => {
            match turtl.read().unwrap().remote_send(format!(r#"{{"e":"unknown_command","cmd":"{}"}}"#, cmd)) {
                Err(e) => error!("dispatch -- problem sending error message: {}", e),
                _ => ()
            }
            Err(TError::Msg(format!("bad command: {}", cmd)))
        }
    };
    match res {
        Ok(..) => (),
        Err(e) => match e {
            _ => error!("dispatch: error processing message: {}", e),
        },
    };
    Ok(())
}

