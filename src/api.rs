//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::sync::RwLock;
use ::std::io::Read;

use ::config;
use ::hyper;
use ::hyper::method::Method;
use ::hyper::header::Headers;
pub use ::hyper::status::StatusCode as Status;
use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::crypto;

/// Holds our Api configuration. This consists of any mutable fields the Api
/// needs to build URLs or make decisions.
struct ApiConfig {
    auth: Option<String>,
}

impl ApiConfig {
    /// Create a new, blank config
    fn new() -> ApiConfig {
        ApiConfig {
            auth: None,
        }
    }
}

/// Our Api object. Responsible for making outbound calls to our Turtl server.
pub struct Api {
    config: RwLock<ApiConfig>,
}

impl Api {
    /// Create an Api
    pub fn new() -> Api {
        Api {
            config: RwLock::new(ApiConfig::new()),
        }
    }

    /// Set the API's authentication
    pub fn set_auth(&self, auth: String) -> TResult<()> {
        let auth_str = String::from("user:") + &auth;
        let base_auth = try!(crypto::to_base64(&Vec::from(auth_str.as_bytes())));
        let ref mut config_guard = self.config.write().unwrap();
        config_guard.auth = Some(String::from("Basic ") + &base_auth);
        Ok(())
    }

    /// Clear out the API auth
    pub fn clear_auth(&self) {
        let ref mut config_guard = self.config.write().unwrap();
        config_guard.auth = None;
    }

    /// Send out an API request
    pub fn call(&self, method: Method, resource: &str, data: Value) -> TResult<Value> {
        info!("api::call() -- req: {} {}", method, resource);
        let endpoint = try!(config::get::<String>(&["api", "endpoint"]));
        let mut url = String::with_capacity(endpoint.len() + resource.len());
        url.push_str(&endpoint[..]);
        url.push_str(resource);
        let auth = {
            let ref guard = self.config.read().unwrap();
            guard.auth.clone()
        };
        let resource = String::from(resource);
        let method2 = method.clone();

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
    }

    /// Convenience function for api.call(GET)
    pub fn get(&self, resource: &str, data: Value) -> TResult<Value> {
        self.call(Method::Get, resource, data)
    }

    /// Convenience function for api.call(POST)
    pub fn post(&self, resource: &str, data: Value) -> TResult<Value> {
        self.call(Method::Post, resource, data)
    }

    /// Convenience function for api.call(PUT)
    #[allow(dead_code)]
    pub fn put(&self, resource: &str, data: Value) -> TResult<Value> {
        self.call(Method::Put, resource, data)
    }

    /// Convenience function for api.call(DELETE)
    #[allow(dead_code)]
    pub fn delete(&self, resource: &str, data: Value) -> TResult<Value> {
        self.call(Method::Delete, resource, data)
    }
}

