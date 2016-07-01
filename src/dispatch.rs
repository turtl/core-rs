use ::error::{TResult, TError};
use ::messaging;
use ::util::json;
use ::util::json::Value;
use ::models::user;

pub fn process(msg: &String) -> TResult<()> {
    let data: Value = try_t!(json::parse(msg));

    // grab the command from the data
    let cmd = try_t!(json::find_string(&["0"], &data));

    match cmd.as_ref() {
        "user:login" => {
            let username = try_t!(json::find_string(&["1", "username"], &data));
            let password = try_t!(json::find_string(&["1", "password"], &data));
            user::login(username.to_owned(), password.to_owned())
        },
        "ping" => {
            info!("ping!");
            return messaging::send(&"{\"e\":\"pong\"}".to_owned())
        }
        "shutdown" => return Err(TError::Shutdown),
        _ => Err(TError::Msg(format!("bad command: {}", cmd))),
    }
}

pub fn main() {
    match messaging::bind(&process) {
        Ok(..) => (),
        Err(e) => panic!("dispatch: error starting messaging system: {}", e),
    }
}

