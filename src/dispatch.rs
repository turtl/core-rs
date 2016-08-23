use ::futures::Future;

use ::error::{TResult, TError};
use ::util::{self, json};
use ::util::json::Value;
use ::turtl::TurtlWrap;
use ::models::user::User;
use ::stop;

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
                stop();
            }
            _ => error!("dispatch: error processing message: {}", e),
        },
    };
}

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: TurtlWrap, msg: &String) -> TResult<()> {
    let data: Value = try_t!(json::parse(msg));

    // grab the command from the data
    let cmd: String = try_t!(json::get(&["0"], &data));

    let res = match cmd.as_ref() {
        "user:login" => {
            let username = try_t!(json::get(&["1", "username"], &data));
            let password = try_t!(json::get(&["1", "password"], &data));
            User::login(turtl.clone(), username, password)
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

