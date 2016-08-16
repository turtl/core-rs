use ::std::sync::mpsc::Sender;

use ::util::reqres::{Request, Response};
use ::error::TResult;

pub fn dispatch(tx: Sender<Response>, req: Request) -> TResult<()> {
    Ok(())
}

