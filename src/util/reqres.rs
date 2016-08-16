//! The purpose of the ReqRes module is to make it easier to send requests off
//! to separate threads and track the responses that come back. For instance,
//! you might want to run an SQLite query and get a result...ReqRes tracks the
//! request and matches the appropriate response to it, calling the associated
//! callback when the response comes back.

use std::sync::RwLock;

use ::error::{TResult, TError};
use ::std::collections::HashMap;

/// Stores state about our Request/Response system
pub struct ReqRes {
    /// holds the current (unique) request id
    id: u64,
    /// holds all request -> response mappings
    tracker: HashMap<u64, Box<Fn(Vec<u8>)>>,
}

/// Our Request struct is fairly simple...it tracks the request id and the
/// actual string request we're sending.
pub struct Request {
    pub id: u64,
    pub request: String,
}

/// Let's us know what kind of type a response is
pub enum ResponseType {
    String,
    Data,
    Error,
}

/// Our response, like Request, has an id and a field for data. Note that data
/// is a byte vector instead of string. This is the best primitive for
/// representing data.
pub struct Response {
    id: u64,
    data: Vec<u8>,
    response_type: ResponseType,
}

impl ReqRes {
    /// Create a new (empty) ReqRes
    pub fn new() -> ReqRes {
        ReqRes {
            id: 0,
            tracker: HashMap::new(),
        }
    }

    /// Create a new request object
    pub fn request<F>(&mut self, request: String, cb: F) -> Request
        where F: Fn(Vec<u8>) + 'static
    {
        self.id += 1;
        self.tracker.insert(self.id, Box::new(cb));
        Request::new(self.id, request)
    }

    pub fn respond(&mut self, response: Response) -> TResult<()> {
        let id = response.id;
        let data = response.data;
        if !self.tracker.contains_key(&id) { return Err(TError::MissingData(format!("reqres: missing callback for request {}", id))); }
        (self.tracker.get(&id).unwrap())(data);
        self.tracker.remove(&id);
        Ok(())
    }
}

impl Request {
    /// Create a new Request and give it a unique id
    pub fn new(id: u64, request: String) -> Request {
        Request {
            id: id,
            request: request,
        }
    }

    /// Create a Response object from this Request
    pub fn response(&self, response_type: ResponseType, data: Vec<u8>) -> Response {
        Response::new(self.id, response_type, data)
    }
}

impl Response {
    /// Create a new response manually
    pub fn new(id: u64, response_type: ResponseType, data: Vec<u8>) -> Response {
        Response {
            id: id,
            response_type: response_type,
            data: data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    #[test]
    fn req_res() {
        let data = Arc::new(RwLock::new(vec![0, 0]));
        let rdata = data.clone();

        {
            let request_sql = String::from("SELECT id FROM users WHERE name = 'slappy';");
            let data_sql = data.clone();
            let cb_sql = move |data: Vec<u8>| {
                let res = String::from_utf8(data).unwrap();
                data_sql.write().unwrap()[0] += 1;
                assert_eq!(res, "6969");
            };

            let request_api = String::from("/users/friends");
            let data_api = data.clone();
            let cb_api = move |data: Vec<u8>| {
                let res = String::from_utf8(data).unwrap();
                data_api.write().unwrap()[1] += 1;
                assert_eq!(res, r#"{"friends":[]}"#);
            };

            let mut reqres = ReqRes::new();

            let req_sql = reqres.request(request_sql, cb_sql);
            let req_api = reqres.request(request_api, cb_api);
            let req_sql_id = req_sql.id;
            let req_api_id = req_api.id;

            let res_sql = req_sql.response(ResponseType::String, Vec::from(String::from("6969").as_bytes()));
            let res_api = req_api.response(ResponseType::String, Vec::from(String::from(r#"{"friends":[]}"#).as_bytes()));

            assert_eq!(rdata.read().unwrap()[0], 0);
            assert_eq!(rdata.read().unwrap()[1], 0);

            reqres.respond(res_sql).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 1);
            assert_eq!(rdata.read().unwrap()[1], 0);
            match reqres.respond(Response::new(req_sql_id, ResponseType::String, Vec::new())) {
                Ok(..) => panic!("reqres: double-responded to request"),
                Err(..) => (),
            }

            reqres.respond(res_api).unwrap();
            assert_eq!(rdata.read().unwrap()[0], 1);
            assert_eq!(rdata.read().unwrap()[1], 1);
            match reqres.respond(Response::new(req_api_id, ResponseType::String, Vec::new())) {
                Ok(..) => panic!("reqres: double-responded to request"),
                Err(..) => (),
            }
        }
    }
}


