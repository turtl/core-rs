use ::error::{TResult, TError};
use ::util::{self, json};
use ::util::json::Value;
use ::turtl::Turtl;
use ::stop;

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: &mut Turtl, msg: &String) -> TResult<()> {
    let data: Value = try_t!(json::parse(msg));

    // grab the command from the data
    let cmd: String = try_t!(json::get(&["0"], &data));

    let res = match cmd.as_ref() {
        "user:login" => {
            let username = try_t!(json::get(&["1", "username"], &data));
            let password = try_t!(json::get(&["1", "password"], &data));
            turtl.user.login(username, password)
        },
        "ping" => {
            info!("ping!");
            return turtl.remote_send("{\"e\":\"pong\"}".to_owned())
        }
        "shutdown" => return Err(TError::Shutdown),
        _ => Err(TError::Msg(format!("bad command: {}", cmd))),
    };
    match res {
        Ok(..) => (),
        Err(e) => match e {
            TError::Msg(_) | TError::BadValue(_) | TError::MissingField(_) | TError::MissingData(_) | TError::TryAgain
                => error!("dispatch: error processing message: {}", e),
            TError::Shutdown => {
                warn!("dispatch: got shutdown signal, quitting");
                util::sleep(10);
                match turtl.remote_send("{\"e\":\"shutdown\"}".to_owned()) {
                    Ok(..) => (),
                    Err(..) => (),
                }
                util::sleep(10);
                stop();
            }
        },
    };
    Ok(())
}

