//! The ReqRes module builds on the Event module to make actions that follow the
//! request/response paradigm a bit easier.
//!
//! Each request is given a unique id and responses with that id are matched to
//! the original request when received.

use std::sync::RwLock;

use ::error::{TResult, TError};
use ::util::event::{EventEmitter};
use ::util::json::Value;

lazy_static! {
    /// track our current request id
    static ref req_id: RwLock<u64> = RwLock::new(0);
}

/// Our Request struct is fairly simple...it tracks the request id and the
/// actual string request we're sending.
pub struct Request {
    id: u64,
    request: String,
}

/// Our response, like Request, has an id and a field for data. Note that data
/// is a byte vector instead of string. This is the best primitive for
/// representing data.
pub struct Response {
    id: u64,
    data: Vec<u8>
}

impl Request {
    /// Create a new Request and give it a unique id
    fn new(request: String) -> TResult<Request> {
        let mut id = try_t!((*req_id).write());
        *id += 1;
        Ok(Request {
            id: *id,
            request: request,
        })
    }

    /// Create a Response object from this Request
    fn response(&self, data: Vec<u8>) -> Response {
        Response::new(self.id, data)
    }
}

impl Response {
    /// Create a new response manually
    fn new(id: u64, data: Vec<u8>) -> Response {
        Response {
            id: id,
            data: data,
        }
    }
}

/*
pub fn bind(emitter: &mut EventEmitter, request: String, cb: F)
    where F: Fn(Vec<u8>) + 'static
{
    let reqobj = Request::new(request);
    let name = format!("reqres:bound:{}:", &reqobj.id);
    emitter.bind(&name[..], ,&name[..]);
}
*/

