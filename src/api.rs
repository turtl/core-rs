use ::hyper;
use ::std::io::Read;

use ::error::{TResult, TError};
use ::util::json::{self, Value};

pub struct Api {
    endpoint: String,
    auth: Option<String>,
}

impl Api {
    pub fn new(endpoint: String) -> Api {
        Api {
            endpoint: endpoint,
            auth: None,
        }
    }

    pub fn get(&self, resource: &str) -> TResult<Value> {
        let client = hyper::Client::new();
        let mut out = String::new();
        let url = format!("{}{}", self.endpoint, resource);
        let mut res = try_t!(client.get(&url[..]).send());
        try_t!(res.read_to_string(&mut out));
        Ok(try_t!(json::parse(&out)))
    }
}

