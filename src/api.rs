use ::std::io::Read;

use ::hyper;
use ::hyper::method::Method;
use ::futures::{self, Oneshot};

use ::error::{TResult, TError};
use ::util::json::{self, Value};
use ::util::thredder::{Thredder, OpData, Pipeline};

pub struct Api {
    endpoint: String,
    auth: Option<String>,
    thredder: Thredder,
}

impl Api {
    pub fn new(endpoint: String, tx_main: Pipeline) -> Api {
        Api {
            endpoint: endpoint,
            auth: None,
            thredder: Thredder::new("api", tx_main, 1),
        }
    }

    pub fn set_endpoint(&mut self, endpoint: String) {
        self.endpoint = endpoint;
    }

    pub fn call(&self, method: Method, resource: &str) -> Oneshot<TResult<Value>> {
        let (tx, rx) = futures::oneshot();
        let mut url = String::with_capacity(self.endpoint.len() + resource.len());
        url.push_str(&self.endpoint[..]);
        url.push_str(resource);
        self.thredder.run(move || {
            let client = hyper::Client::new();
            let req = client.request(method, &url[..]);
            req.send().map_err(|e| toterr!(e))
                .and_then(|mut res| {
                    let mut out = String::new();
                    res.read_to_string(&mut out)
                        .map_err(|e| toterr!(e))
                        .and_then(move |_| Ok(out))
                })
                .and_then(|out| json::parse::<Value>(&out).map_err(|e| toterr!(e)))
        }, move |data: TResult<OpData>| {
            tx.complete(OpData::to_value(data));
        });
        rx
    }

    pub fn get(&self, resource: &str) -> Oneshot<TResult<Value>> {
        self.call(Method::Get, resource)
    }
}

