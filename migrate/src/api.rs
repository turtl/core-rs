//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::io::Read;
use ::std::time::Duration;
use ::config;
use ::reqwest::{Method, blocking::RequestBuilder, blocking::Client, Url, Proxy};
use ::reqwest::header::{HeaderMap, HeaderValue};
pub use ::reqwest::StatusCode;
use ::jedi::{self, Value, DeserializeOwned};
use ::error::{MResult, MError};
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
    headers: HeaderMap,
    timeout: Duration,
    data: Value,
}

impl ApiReq {
    /// Create a new builder
    pub fn new() -> Self {
        ApiReq {
            headers: HeaderMap::new(),
            timeout: Duration::new(10, 0),
            data: Value::Null,
        }
    }

    /// Set a header
    #[allow(dead_code)]
    pub fn header<'a>(mut self, name: &'static str, val: &String) -> Self {
        self.headers.insert(name, HeaderValue::from_str(val.as_str()).expect("ApiReq.header() -- bad header value given"));
        self
    }

    /// Set (override) the timeout for this request
    pub fn timeout<'a>(mut self, secs: u64) -> Self {
        self.timeout = Duration::new(secs, 0);
        self
    }

    /// Set this request's data
    #[allow(dead_code)]
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
    config: ApiConfig,
}

impl Api {
    /// Create an Api
    pub fn new() -> Api {
        Api {
            config: ApiConfig::new(),
        }
    }

    /// Set the API's authentication
    pub fn set_auth(&mut self, auth: String) -> MResult<()> {
        let auth_str = String::from("user:") + &auth;
        let base_auth = crypto::to_base64(&Vec::from(auth_str.as_bytes()))?;
        self.config.auth = Some(String::from("Basic ") + &base_auth);
        Ok(())
    }

    /// Grab the auth our of the API object
    pub fn get_auth(&self) -> Option<String> {
        self.config.auth.as_ref().map(|x| x.clone())
    }

    /// Write our auth headers into a header collection
    pub fn set_auth_headers(&self, req: RequestBuilder) -> RequestBuilder {
        match self.config.auth.as_ref() {
            Some(x) => req.header("Authorization", x.clone()),
            None => req,
        }
    }

    /// Set our standard auth header into a Headers set
    fn set_standard_headers(&self, req: RequestBuilder) -> RequestBuilder {
        self.set_auth_headers(req)
            .header("Content-Type", "application/json")
    }

    /// Build a full URL given a resource
    fn build_url(&self, resource: &str) -> MResult<String> {
        let endpoint = config::get::<String>(&["api", "v6", "endpoint"])?;
        let mut url = String::with_capacity(endpoint.len() + resource.len());
        url.push_str(endpoint.trim_end_matches('/'));
        url.push_str(resource);
        Ok(url)
    }

    /// Send out an API request
    pub fn call<T: DeserializeOwned>(&self, method: Method, resource: &str, builder: ApiReq) -> MResult<T> {
        debug!("api::call() -- req: {} {}", method, resource);
        let ApiReq {headers, timeout, data} = builder;
        let url = self.build_url(resource)?;
        let mut client_builder = Client::builder()
            .timeout(timeout);
        match config::get::<Option<String>>(&["api", "proxy"]) {
            Ok(x) => {
                if let Some(proxy_cfg) = x {
                    client_builder = client_builder.proxy(Proxy::http(format!("http://{}", proxy_cfg).as_str())?);
                }
            }
            Err(_) => {}
        }
        let client = client_builder.build()?;
        let req = client.request(method, Url::parse(url.as_str())?);
        let req = self.set_standard_headers(req)
            .headers(headers)
            .json(&data)
            .build()?;
        let callinfo = CallInfo::new(req.method().clone(), String::from(req.url().as_str()));
        let res = client.execute(req);
        res
            .map_err(|e| { tomerr!(e) })
            .and_then(|mut res| {
                let mut out = String::new();
                let str_res = res.read_to_string(&mut out)
                    .map_err(|e| tomerr!(e))
                    .and_then(move |_| Ok(out));
                if !res.status().is_success() {
                    let errstr = match str_res {
                        Ok(x) => x,
                        Err(e) => {
                            error!("api::call() -- problem grabbing error message: {}", e);
                            String::from("<unknown>")
                        }
                    };
                    return Err(MError::Api(res.status(), errstr));
                }
                str_res.map(move |x| (x, res))
            })
            .map(|(out, res)| {
                info!("api::call() -- res({}): {:?} {} {}", out.len(), res.status().as_u16(), &callinfo.method, &callinfo.resource);
                trace!("  api::call() -- body: {}", out);
                out
            })
            .and_then(|out| jedi::parse(&out).map_err(|e| tomerr!(e)))
    }

    /// Convenience function for api.call(GET)
    pub fn get<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> MResult<T> {
        self.call(Method::GET, resource, builder)
    }

    /// Convenience function for api.call(POST)
    pub fn post<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> MResult<T> {
        self.call(Method::POST, resource, builder)
    }

    /// Convenience function for api.call(PUT)
    #[allow(dead_code)]
    pub fn put<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> MResult<T> {
        self.call(Method::PUT, resource, builder)
    }

    /// Convenience function for api.call(DELETE)
    #[allow(dead_code)]
    pub fn delete<T: DeserializeOwned>(&self, resource: &str, builder: ApiReq) -> MResult<T> {
        self.call(Method::DELETE, resource, builder)
    }
}

