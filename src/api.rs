//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::sync::RwLock;
use ::std::io::Read;
use ::std::time::Duration;

use ::config;
use ::hyper;
pub use ::hyper::method::Method;
use ::hyper::client::request::Request;
use ::hyper::client::response::Response;
use ::hyper::header;
pub use ::hyper::header::Headers;
pub use ::hyper::status::StatusCode as Status;
use ::jedi::{self, Value, DeserializeOwned};

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

/// Used to store some info we want when we send a response to call_end()
pub struct CallInfo {
    method: Method,
    resource: String,
}

impl CallInfo {
    /// Create a new call info object
    fn new(method: Method, resource: String) -> Self {
        Self {
            method: method,
            resource: resource,
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
    pub fn set_auth(&self, username: String, auth: String) -> TResult<()> {
        let auth_str = format!("{}:{}", username, auth);
        let base_auth = crypto::to_base64(&Vec::from(auth_str.as_bytes()))?;
        let ref mut config_guard = lockw!(self.config);
        config_guard.auth = Some(String::from("Basic ") + &base_auth);
        Ok(())
    }

    /// Clear out the API auth
    pub fn clear_auth(&self) {
        let ref mut config_guard = lockw!(self.config);
        config_guard.auth = None;
    }

    /// Write our auth headers into a header collection
    pub fn set_auth_headers(&self, headers: &mut Headers) {
        let auth = {
            let ref guard = lockr!(self.config);
            guard.auth.clone()
        };
        match auth {
            Some(x) => headers.set_raw("Authorization", vec![Vec::from(x.as_bytes())]),
            None => (),
        }
    }

    /// Set our standard auth header into a Headers set
    fn set_standard_headers(&self, headers: &mut Headers) {
        self.set_auth_headers(headers);
        if headers.get_raw("Content-Type").is_none() {
            headers.set(header::ContentType::json());
        }
    }

    /// Build a full URL given a resource
    fn build_url(&self, resource: &str) -> TResult<String> {
        let endpoint = config::get::<String>(&["api", "endpoint"])?;
        let mut url = String::with_capacity(endpoint.len() + resource.len());
        url.push_str(&endpoint[..]);
        url.push_str(resource);
        Ok(url)
    }

    /// Start an API request. call_start()/call_end() can be used to stream a
    /// large HTTP body
    pub fn call_start(&self, method: Method, resource: &str, builder: ApiReq) -> TResult<(Request<hyper::net::Streaming>, CallInfo)> {
        debug!("api::call_start() -- req: {} {}", method, resource);
        let ApiReq {mut headers, timeout, data: _data} = builder;
        let url = self.build_url(resource)?;
        let resource = String::from(resource);
        let method2 = method.clone();
        self.set_standard_headers(&mut headers);
        let mut request = Request::new(method, hyper::Url::parse(&url[..])?)?;
        request.set_read_timeout(Some(timeout))?;
        {
            // ridiculous. there has to be a better way??
            let reqheaders = request.headers_mut();
            for header in headers.iter() {
                let name_string = String::from(header.name());
                reqheaders.set_raw(name_string, vec![Vec::from(header.value_string().as_bytes())]);
            }
        }
        Ok((request.start()?, CallInfo::new(method2, resource)))
    }

    /// Send out an API request
    pub fn call<T: DeserializeOwned>(&self, method: Method, resource: &str, builder: ApiReq) -> TResult<T> {
        debug!("api::call() -- req: {} {}", method, resource);
        let ApiReq {mut headers, timeout, data} = builder;
        let url = self.build_url(resource)?;
        let resource = String::from(resource);
        let method2 = method.clone();

        let mut client = hyper::Client::new();
        let body = jedi::stringify(&data)?;
        self.set_standard_headers(&mut headers);
        client.set_read_timeout(Some(timeout));
        let res = client
            .request(method, &url[..])
            .body(&body)
            .headers(headers)
            .send();
        self.call_end(res, CallInfo::new(method2, resource))
    }

    /// Finish an API request (takes a response result given back by
    /// Request.send())
    pub fn call_end<T: DeserializeOwned>(&self, response: Result<Response, hyper::error::Error>, callinfo: CallInfo) -> TResult<T> {
        response
            .map_err(|e| {
                match e {
                    hyper::Error::Io(err) => twrap!(TError::Io(err)),
                    _ => toterr!(e),
                }
            })
            .and_then(|mut res| {
                let mut out = String::new();
                let str_res = res.read_to_string(&mut out)
                    .map_err(|e| toterr!(e))
                    .and_then(move |_| Ok(out));
                if !res.status.is_success() {
                    let errstr = match str_res {
                        Ok(x) => x,
                        Err(e) => {
                            error!("api::call() -- problem grabbing error message: {}", e);
                            String::from("<unknown>")
                        }
                    };
                    return TErr!(TError::Api(res.status, errstr));
                }
                str_res.map(move |x| (x, res))
            })
            .map(|(out, res)| {
                info!("api::call() -- res({}): {:?} {} {}", out.len(), res.status_raw(), &callinfo.method, &callinfo.resource);
                trace!("  api::call() -- body: {}", out);
                out
            })
            .and_then(|out| jedi::parse(&out).map_err(|e| toterr!(e)))
    }

    /// Convenience function for api.call(GET)
    pub fn get<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> TResult<T> {
        self.call(Method::Get, resource, builder)
    }

    /// Convenience function for api.call(POST)
    pub fn post<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> TResult<T> {
        self.call(Method::Post, resource, builder)
    }

    /// Convenience function for api.call(PUT)
    pub fn put<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> TResult<T> {
        self.call(Method::Put, resource, builder)
    }

    /// Convenience function for api.call(DELETE)
    pub fn delete<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> TResult<T> {
        self.call(Method::Delete, resource, builder)
    }
}

