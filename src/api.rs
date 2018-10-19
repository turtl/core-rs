//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::sync::RwLock;
use ::std::io::Read;
use ::std::time::Duration;
use ::config;
use ::jedi::{self, Value, DeserializeOwned, Serialize};
use ::error::{TResult, TError};
use ::crypto;
use ::reqwest::{self, RequestBuilder, Client, Url, Proxy};
pub use ::reqwest::Method;
pub use ::reqwest::StatusCode;

/// Pull out our crate version to send to the api
const CORE_VERSION: &'static str = env!("CARGO_PKG_VERSION");

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
    timeout: Duration,
}

impl ApiReq {
    /// Create a new builder
    pub fn new() -> Self {
        ApiReq {
            timeout: Duration::new(10, 0),
        }
    }

    /// Set (override) the timeout for this request
    pub fn timeout<'a>(mut self, secs: u64) -> Self {
        self.timeout = Duration::new(secs, 0);
        self
    }
}

/// Wraps calling the Turtl API in an object
pub struct ApiCaller {
    req: RequestBuilder,
}

impl ApiCaller {
    fn from_req(req: RequestBuilder) -> ApiCaller {
        ApiCaller { req: req }
    }

    pub fn header<T: Into<String>>(self, name: &str, val: T) -> Self {
        ApiCaller::from_req(self.req.header(name, val.into()))
    }

    pub fn body<T: Into<reqwest::Body>>(self, body: T) -> Self {
        ApiCaller::from_req(self.req.body(body))
    }

    pub fn json<T: Serialize + ?Sized>(self, json: &T) -> Self {
        ApiCaller::from_req(self.req.json(json))
    }

    #[allow(dead_code)]
    pub fn query<T: Serialize + ?Sized>(self, query: &T) -> Self {
        ApiCaller::from_req(self.req.query(query))
    }

    #[allow(dead_code)]
    pub fn form<T: Serialize + ?Sized>(self, form: &T) -> Self {
        ApiCaller::from_req(self.req.form(form))
    }

    pub fn call<T: DeserializeOwned>(self) -> TResult<T> {
        self.call_opt_impl(None)
    }

    pub fn call_opt<T: DeserializeOwned>(self, apireq: ApiReq) -> TResult<T> {
        self.call_opt_impl(Some(apireq))
    }

    pub fn call_opt_impl<T: DeserializeOwned>(self, builder_maybe: Option<ApiReq>) -> TResult<T> {
        let mut client_builder = Client::builder();
        if let Some(builder) = builder_maybe {
            let ApiReq { timeout } = builder;
            client_builder = client_builder.timeout(timeout);
        }
        match config::get::<Option<String>>(&["api", "proxy"]) {
            Ok(x) => {
                if let Some(proxy_cfg) = x {
                    debug!("api::call() -- req: using proxy: {}", proxy_cfg);
                    client_builder = client_builder.proxy(Proxy::http(format!("http://{}", proxy_cfg).as_str())?);
                }
            }
            Err(_) => {}
        }
        let client = client_builder.build()?;
        let ApiCaller { req: reqb } = self;
        let req = reqb.build()?;
        let callinfo = CallInfo::new(req.method().clone(), String::from(req.url().as_str()));
        debug!("api::call() -- req: {} {}", req.method(), req.url());
        let res = client.execute(req);
        res
            .map_err(|e| { toterr!(e) })
            .and_then(|mut res| {
                let mut out = String::new();
                let str_res = res.read_to_string(&mut out)
                    .map_err(|e| toterr!(e))
                    .and_then(move |_| Ok(out));
                if !res.status().is_success() {
                    let errstr = match str_res {
                        Ok(x) => x,
                        Err(e) => {
                            error!("api::call() -- problem grabbing error message: {}", e);
                            String::from("<unknown>")
                        }
                    };
                    let val = match jedi::parse(&errstr) {
                        Ok(x) => x,
                        Err(_) => Value::String(errstr),
                    };
                    return TErr!(TError::Api(res.status(), val));
                }
                str_res.map(move |x| (x, res))
            })
            .map(|(out, res)| {
                info!("api::call() -- res({}): {:?} {} {}", out.len(), res.status().as_u16(), &callinfo.method, &callinfo.resource);
                trace!("  api::call() -- body: {}", out);
                out
            })
            .map_err(|err| {
                debug!("api::call() -- call error: {}", err);
                err
            })
            .and_then(|out| {
                jedi::parse(&out).map_err(|e| {
                    warn!("api::call() -- JSON parse error: {}", out);
                    toterr!(e)
                })
            })
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
    pub fn set_auth_headers(&self, req: RequestBuilder) -> RequestBuilder {
        let auth = {
            let ref guard = lockr!(self.config);
            guard.auth.clone()
        };
        match auth {
            Some(x) => {
                req.header("Authorization", x)
            },
            None => req
        }
    }

    /// Set our standard auth header into a Headers set
    fn set_standard_headers(&self, req: RequestBuilder) -> RequestBuilder {
        let req = self.set_auth_headers(req)
            .header("Content-Type", "application/json");
        match config::get::<String>(&["api", "client_version_string"]) {
            Ok(version) => {
                let header_val = format!("{}/{}", version, CORE_VERSION);
                req.header("X-Turtl-Client", header_val)
            }
            Err(_) => req,
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

    /// Given a method an url, return a Reqwest RequestBuilder
    pub fn req(&self, method: Method, resource: &str) -> TResult<ApiCaller> {
        debug!("api::req() -- begin: {} {}", method, resource);
        let url = self.build_url(resource)?;
        let req = Client::new().request(method, Url::parse(url.as_str())?);
        Ok(ApiCaller::from_req(self.set_standard_headers(req)))
    }

    /// Convenience function for api.call(GET)
    pub fn get(&self, resource: &str) -> TResult<ApiCaller> {
        self.req(Method::GET, resource)
    }

    /// Convenience function for api.call(POST)
    pub fn post(&self, resource: &str) -> TResult<ApiCaller> {
        self.req(Method::POST, resource)
    }

    /// Convenience function for api.call(PUT)
    pub fn put(&self, resource: &str) -> TResult<ApiCaller> {
        self.req(Method::PUT, resource)
    }

    /// Convenience function for api.call(DELETE)
    pub fn delete(&self, resource: &str) -> TResult<ApiCaller> {
        self.req(Method::DELETE, resource)
    }
}

