//! The Api system is responsible for talking to our Turtl server, and manages
//! our user authentication.

use ::std::sync::{RwLock, Mutex};
use ::std::io::Read;
use ::std::time::Duration;
use ::std::collections::HashMap;
use ::config;
use ::jedi::{self, Value, DeserializeOwned, Serialize};
use ::error::{TResult, TError};
use ::crypto;
use ::reqwest::{self, blocking::RequestBuilder, blocking::Client, Url, Proxy};
pub use ::reqwest::Method;
pub use ::reqwest::StatusCode;

/// Pull out our crate version to send to the api
const CORE_VERSION: &'static str = env!("CARGO_PKG_VERSION");

lazy_static! {
    /// A hash table that holds HTTP clients. we used to just create/destroy
    /// clients on each request, but that exhausts connections so it's better to
    /// cache the clients and let them use their internal connection pool.
    static ref CLIENTS: Mutex<HashMap<String, Client>> = Mutex::new(HashMap::new());
}

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

    pub fn body<T: Into<reqwest::blocking::Body>>(self, body: T) -> Self {
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
        let mut cachekey: Vec<String> = Vec::with_capacity(2);
        let mut client_builder = Client::builder();
        if let Some(builder) = builder_maybe {
            let ApiReq { timeout } = builder;
            client_builder = client_builder.timeout(timeout);
            cachekey.push(format!("timeout-{}", timeout.as_secs()));
        }
        match config::get::<Option<String>>(&["api", "proxy"]) {
            Ok(x) => {
                if let Some(proxy_cfg) = x {
                    debug!("api::call() -- req: using proxy: {}", proxy_cfg);
                    let proxystr = format!("{}", proxy_cfg);
                    cachekey.push(format!("proxy-{}", proxystr));
                    client_builder = client_builder.proxy(Proxy::all(proxystr.as_str())?);
                }
            }
            Err(_) => {}
        }
        match config::get::<Option<bool>>(&["api", "allow_invalid_ssl"]) {
            Ok(x) => {
                if let Some(allow_invalid_ssl) = x {
                    if allow_invalid_ssl {
                        debug!("api::call() -- req: allow invalid ssl");
                        cachekey.push(String::from("allow-invalid-ssl"));
                        client_builder = client_builder.danger_accept_invalid_certs(true);
                    }
                }
            }
            Err(_) => {}
        }
        let cachekey_string: String = cachekey.join("///");
        let client = {
            let mut client_guard = lock!((*CLIENTS));
            if !client_guard.contains_key(&cachekey_string) {
                let client = client_builder.build()?;
                debug!("api::call() -- creating new client with cachekey {}", cachekey_string);
                client_guard.insert(cachekey_string.clone(), client);
            }
            // notice we clone here...the client lets us clone without messing
            // up the pooling. very nice!
            client_guard.get(&cachekey_string).unwrap().clone()
        };
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
        url.push_str(endpoint.trim_end_matches('/'));
        url.push_str(resource);
        Ok(url)
    }

    /// Given a method an url, return a Reqwest RequestBuilder
    pub fn req(&self, method: Method, resource: &str) -> TResult<ApiCaller> {
        debug!("api::req() -- begin: {} {}", method, resource);
        let url = self.build_url(resource)?;
        let req = Client::builder().build()?.request(method, Url::parse(url.as_str())?);
        trace!("api::req() -- made client, got req: {:?}", req);
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

