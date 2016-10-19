use ::std::io::Read;

use ::config;
use ::hyper;
use ::hyper::method::Method;
use ::hyper::header::Headers;
pub use ::hyper::status::StatusCode as Status;
use ::jedi::{self, Value};
use ::futures::{self, Future};

use ::error::{TResult, TFutureResult, TError};
use ::util::thredder::{Thredder, Pipeline};
use ::crypto;

pub struct Api {
    auth: Option<String>,
    thredder: Thredder,
}

impl Api {
    pub fn new(tx_main: Pipeline) -> Api {
        Api {
            auth: None,
            thredder: Thredder::new("api", tx_main, 1),
        }
    }

    /// Set the API's authentication
    pub fn set_auth(&mut self, auth: String) -> TResult<()> {
        let auth_str = String::from("user:") + &auth;
        let base_auth = try!(crypto::to_base64(&Vec::from(auth_str.as_bytes())));
        self.auth = Some(String::from("Basic ") + &base_auth);
        Ok(())
    }

    /// Clear out the API auth
    pub fn clear_auth(&mut self) {
        self.auth = None;
    }

    /// Send out an API request
    pub fn call(&self, method: Method, resource: &str, data: Value) -> TFutureResult<Value> {
        info!("api::call() -- req: {} {}", method, resource);
        let endpoint = match config::get::<String>(&["api", "endpoint"]) {
            Ok(x) => x,
            Err(e) => return futures::failed(From::from(e)).boxed(),
        };
        let mut url = String::with_capacity(endpoint.len() + resource.len());
        url.push_str(&endpoint[..]);
        url.push_str(resource);
        let auth = match &self.auth {
            &Some(ref x) => Some(String::from(&x[..])),
            &None => None
        };
        let resource = String::from(resource);
        let method2 = method.clone();
        self.thredder.run(move || {
            let client = hyper::Client::new();
            let body = try!(jedi::stringify(&data));
            let mut headers = Headers::new();
            match auth {
                Some(x) => headers.set_raw("Authorization", vec![Vec::from(x.as_bytes())]),
                None => (),
            }
            client.request(method, &url[..])
                .body(&body)
                .headers(headers)
                .send()
                .map_err(|e| toterr!(e))
                .and_then(|mut res| {
                    if !res.status.is_success() {
                        return Err(TError::ApiError(res.status));
                    }
                    let mut out = String::new();
                    res.read_to_string(&mut out)
                        .map_err(|e| toterr!(e))
                        .and_then(move |_| Ok(out))
                })
                .map(|out| {
                    info!("api::call() -- res({}): {} {}", out.len(), method2, resource);
                    out
                })
                .and_then(|out| jedi::parse::<Value>(&out).map_err(|e| toterr!(e)))
        })
    }

    /// Convenience function for api.call(GET)
    pub fn get(&self, resource: &str, data: Value) -> TFutureResult<Value> {
        self.call(Method::Get, resource, data)
    }

    /// Convenience function for api.call(POST)
    pub fn post(&self, resource: &str, data: Value) -> TFutureResult<Value> {
        self.call(Method::Post, resource, data)
    }

    /// Convenience function for api.call(PUT)
    pub fn put(&self, resource: &str, data: Value) -> TFutureResult<Value> {
        self.call(Method::Put, resource, data)
    }

    /// Convenience function for api.call(DELETE)
    pub fn delete(&self, resource: &str, data: Value) -> TFutureResult<Value> {
        self.call(Method::Delete, resource, data)
    }
}

