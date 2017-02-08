//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::sync::RwLock;
use ::std::io::Read;
use ::std::time::Duration;

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

/// A struct used for building API requests
pub struct ApiReq {
    headers: Headers,
    timeout: Duration,
    data: Value,
}

impl ApiReq {
    /// Create a new builder
    pub fn new() -> Self {
        ApiReq {
            headers: Headers::new(),
            timeout: Duration::new(10, 0),
            data: Value::Null,
        }
    }

    /// Set a header
    #[allow(dead_code)]
    pub fn header<'a>(mut self, name: &'static str, val: &String) -> Self {
        self.headers.set_raw(name, vec![Vec::from(val.as_bytes())]);
        self
    }

    /// Set (override) the timeout for this request
    pub fn timeout<'a>(mut self, secs: u64) -> Self {
        self.timeout = Duration::new(secs, 0);
        self
    }

    /// Set this request's data
    pub fn data<'a>(mut self, data: Value) -> Self {
        self.data = data;
        self
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
    pub fn set_auth(&self, username: String, auth: String) -> TResult<()> {
        let auth_str = format!("{}:{}", username, auth);
        let base_auth = crypto::to_base64(&Vec::from(auth_str.as_bytes()))?;
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
    pub fn call(&self, method: Method, resource: &str, builder: ApiReq) -> TResult<Value> {
        info!("api::call() -- req: {} {}", method, resource);
        let ApiReq {mut headers, timeout, data} = builder;
        let endpoint = config::get::<String>(&["api", "endpoint"])?;
        let mut url = String::with_capacity(endpoint.len() + resource.len());
        url.push_str(&endpoint[..]);
        url.push_str(resource);
        let resource = String::from(resource);
        let method2 = method.clone();

        let mut client = hyper::Client::new();
        let body = jedi::stringify(&data)?;
        let auth = {
            let ref guard = self.config.read().unwrap();
            guard.auth.clone()
        };
        match auth {
            Some(x) => headers.set_raw("Authorization", vec![Vec::from(x.as_bytes())]),
            None => (),
        }
        client.set_read_timeout(Some(timeout));
        client
            .request(method, &url[..])
            .body(&body)
            .headers(headers)
            .send()
            .map_err(|e| {
                match e {
                    hyper::Error::Io(err) => TError::Io(err),
                    _ => toterr!(e),
                }
            })
            .and_then(|mut res| {
                if !res.status.is_success() {
                    return Err(TError::Api(res.status));
                }
                let mut out = String::new();
                res.read_to_string(&mut out)
                    .map_err(|e| toterr!(e))
                    .and_then(move |_| Ok(out))
            })
            .map(|out| {
                info!("api::call() -- res({}): {} {}", out.len(), method2, resource);
                trace!("api::call() -- body: {} {} -- {}", method2, resource, out);
                out
            })
            .and_then(|out| jedi::parse::<Value>(&out).map_err(|e| toterr!(e)))
    }

    /// Convenience function for api.call(GET)
    pub fn get(&self, resource: &str, builder: ApiReq) -> TResult<Value> {
        self.call(Method::Get, resource, builder)
    }

    /// Convenience function for api.call(POST)
    pub fn post(&self, resource: &str, builder: ApiReq) -> TResult<Value> {
        self.call(Method::Post, resource, builder)
    }

    /// Convenience function for api.call(PUT)
    #[allow(dead_code)]
    pub fn put(&self, resource: &str, builder: ApiReq) -> TResult<Value> {
        self.call(Method::Put, resource, builder)
    }

    /// Convenience function for api.call(DELETE)
    #[allow(dead_code)]
    pub fn delete(&self, resource: &str, builder: ApiReq) -> TResult<Value> {
        self.call(Method::Delete, resource, builder)
    }
}

