extern crate fern;
extern crate hyper;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate regex;
extern crate scraper;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
extern crate url;

pub mod error;

use ::error::{CResult, CError};
use ::std::env;
use ::std::io::Read;
use ::hyper::method::Method;
use ::hyper::header::Headers;
use ::url::Url;
use ::scraper::{Html, Selector};
use ::regex::Regex;
use ::std::path::PathBuf;
use ::std::fs::File;
use ::jedi::Value;

lazy_static! {
    /// Load our built-in set of custom parsers
    static ref PARSERS: Vec<CustomParser> = {
        let parsers_file = match env::var("CLIPPO_PARSERS") {
            Ok(filename) => {
                PathBuf::from(filename)
            }
            Err(_) => {
                let mut path = env::current_dir().unwrap();
                path.push("parsers.yaml");
                path
            }
        };
        let mut file = match File::open(&parsers_file) {
            Ok(x) => x,
            Err(e) => {
                warn!("Clippo -- error opening `parsers.yaml`: {}", e);
                return vec![];
            },
        };
        let mut contents = String::new();
        match file.read_to_string(&mut contents) {
            Ok(_) => {}
            Err(e) => {
                warn!("Clippo -- error reading `parsers.yaml`: {}", e);
                return vec![];
            }
        }
        match serde_yaml::from_str(&contents) {
            Ok(x) => x,
            Err(e) => {
                warn!("Clippo -- error parsing `parsers.yaml`: {}", e);
                vec![]
            }
        }
    };
}

/// A struct used to tell the bookmarker how to find various pieces of info
/// on a domain
#[derive(Deserialize, Debug)]
pub struct CustomParser {
    /// The domain we're scraping
    domain: String,
    /// A CSS selector used to find the page title
    selector_title: Option<String>,
    /// A CSS selector used to find the page description
    selector_desc: Option<String>,
    /// A CSS selector used to find the page's image
    selector_image: Option<String>,
    /// A regular expression with the named group "json" that returns a block of
    /// JSON we can parse to search for data.
    re_json: Option<String>,
    /// A JSON path for grabbing the page title (used with re_json)
    jpath_title: Option<Vec<String>>,
    /// A JSON path for grabbing the page desc (used with re_json)
    jpath_desc: Option<Vec<String>>,
    /// A JSON path for grabbing the page image (used with re_json)
    jpath_img: Option<Vec<String>>,
    /// A search/replace regular expression to get our image url from the
    /// resource url
    re_image: Option<[String; 2]>,
}

/// A struct that wraps up a bookmark scrape result
#[derive(Serialize, Debug)]
pub struct ClipResult {
    /// The title of the resource we're bookmarking
    title: Option<String>,
    /// The page description of the resource we're bookmarking
    description: Option<String>,
    /// The most prominent image for the url
    image_url: Option<String>,
}

impl ClipResult {
    /// Create a new result from seom data
    pub fn new(title: Option<String>, desc: Option<String>, img: Option<String>) -> Self {
        ClipResult {
            title: title,
            description: desc,
            image_url: img,
        }
    }
}

/// Convert a URL to HTML
fn grab_url(url: &String) -> CResult<String> {
    let client = hyper::Client::new();
    let mut headers = Headers::new();
    fn set_header(headers: &mut Headers, name: &'static str, val: &str) {
        headers.set_raw(name, vec![Vec::from(val.as_bytes())]);
    }
    set_header(&mut headers, "User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:54.0) Gecko/20100101 Firefox/54.0");
    set_header(&mut headers, "Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");
    set_header(&mut headers, "Accept-Language", "en-US,en;q=0.5");
    //set_header(&mut headers, "Accept-Encoding", "");
    set_header(&mut headers, "Cache-Control", "max-age=0");
    let html = client.request(Method::Get, url.as_str())
        .headers(headers)
        .send()
        .map_err(|e| {
            match e {
                hyper::Error::Io(err) => CError::Io(err),
                _ => From::from(e),
            }
        })
        .and_then(|mut res| {
            let mut out = String::new();
            let str_res = res.read_to_string(&mut out)
                .map_err(|e| From::from(e))
                .and_then(move |_| Ok(out));
            if !res.status.is_success() {
                let errstr = match str_res {
                    Ok(x) => x,
                    Err(e) => {
                        error!("api::call() -- problem grabbing error message: {}", e);
                        String::from("<unknown>")
                    }
                };
                return Err(CError::Http(res.status, errstr));
            }
            str_res.map(move |x| x)
        })?;
    Ok(html)
}

/// Given a url, scrape the HTML of the page and try to determine the page
/// title, description, and main image.
pub fn clip(url: &String, parsers: &Vec<CustomParser>) -> CResult<ClipResult> {
    let html = grab_url(url)?;

    /// A helpful function to parse CSS selectors and convert them to CResult
    /// objects. we can't really implement From::from() for selector errors
    /// since the error objects are just (), so we localize the conversion here.
    fn parse_selector(sel: &str) -> CResult<Selector> {
        Selector::parse(sel)
            .map_err(|_| CError::Selector(format!("cannot parse selector {}", sel)))
    }

    // set up our final return objects
    let mut title = None;
    let mut desc = None;
    let mut img = None;

    // set up our selectors
    let mut selector_title = vec![];
    let mut selector_desc = vec![];
    let mut selector_img = vec![];

    macro_rules! json_finder {
        ($jpath:expr, $val:expr, $to:ident) => {
            match $jpath.as_ref() {
                Some(path) => {
                    let arr_str = path.iter()
                        .map(|x| x.as_str())
                        .collect::<Vec<_>>();
                    match jedi::get_opt::<String>(arr_str.as_slice(), &$val) {
                        Some(val) => {
                            $to = Some(val);
                        }
                        None => {}
                    }
                }
                None => {}
            }
        }
    }

    macro_rules! handle_json {
        ($parser:expr, $html:expr) => {
            match $parser.re_json.as_ref() {
                Some(re) => {
                    let json = match Regex::new(re.as_str()) {
                        Ok(rex) => {
                            match rex.captures(html.as_str()) {
                                Some(caps) => {
                                    match caps.name("json") {
                                        Some(mat) => {
                                            Some(String::from(mat))
                                        }
                                        None => { None }
                                    }
                                }
                                None => { None }
                            }

                        }
                        Err(e) => {
                            warn!("clippo::clip() -- bad regex {}: {}", re, e);
                            None
                        }
                    };
                    match json {
                        Some(json) => {
                            match jedi::parse::<Value>(&json) {
                                Ok(val) => {
                                    json_finder!($parser.jpath_title, val, title);
                                    json_finder!($parser.jpath_desc, val, desc);
                                },
                                Err(_) => {
                                    warn!("clippo::clip() -- error parsing JSON returned from re_json block: {}", re);
                                }
                            };
                        }
                        None => {}
                    }
                }
                None => {}
            }
        }
    }

    macro_rules! handle_reimage {
        ($parser:expr, $url:expr) => {
            if img.is_none() {
                match $parser.re_image.as_ref() {
                    Some(re) => {
                        match Regex::new(re[0].as_str()) {
                            Ok(regex) => {
                                let rep = regex.replace_all($url.as_str(), re[1].as_str());
                                if &rep != $url {
                                    img = Some(rep);
                                }
                            }
                            Err(e) => {
                                warn!("clippo::clip() -- bad regex: {}: {}", re[0], e);
                            }
                        }
                    }
                    None => {}
                }
            }
        }
    }

    // a macro to make parsing selectors and pushing them onto our selector list
    // a bit less verbose
    macro_rules! push_selector {
        ($from:expr, $to:ident) => {
            match $from.as_ref() {
                Some(sel) => {
                    match parse_selector(sel.as_str()) {
                        Ok(parsed) => $to.push(parsed),
                        Err(_) => warn!("clippo::clip() -- cannot parse selector {}", sel),
                    }
                }
                None => {}
            }
        }
    }

    // grab our domain from the url and use it to find the parsers we'll be
    // using to grab our info. note that we can pass multiple parsers, and they
    // will be run in the order passed (until the value we want is found).
    let url_parsed = Url::parse(url.as_str())?;
    let domain = url_parsed.domain().unwrap_or("");
    for x in parsers.iter().filter(|x| domain.contains(x.domain.as_str())) {
        handle_json!(x, html);
        handle_reimage!(x, url);
        push_selector!(x.selector_title, selector_title);
        push_selector!(x.selector_desc, selector_desc);
        push_selector!(x.selector_image, selector_img);
    }
    // push our built-in parsers onto our search list
    for x in (*PARSERS).iter().filter(|x| domain.contains(x.domain.as_str())) {
        handle_json!(x, html);
        handle_reimage!(x, url);
        push_selector!(x.selector_title, selector_title);
        push_selector!(x.selector_desc, selector_desc);
        push_selector!(x.selector_image, selector_img);
    }
    // add some default selectors in case we don't have a parser or they turn up
    // blank. keep in mind, it's ok to not have any matches...we'll just return
    // empty strings.
    selector_title.push(parse_selector("head title")?);
    selector_desc.push(parse_selector("head meta[name=\"description\"]")?);
    selector_img.push(parse_selector("meta[property=\"og:image\"]")?);
    selector_img.push(parse_selector("meta[property=\"twitter:image\"]")?);

    let doc = Html::parse_document(html.as_str());
    for sel_title in selector_title {
        if title.is_some() { break; }
        for el in doc.select(&sel_title) {
            if title.is_some() { break; }
            title = Some(String::from(el.inner_html().trim()));
        }
    }
    for sel_desc in selector_desc {
        if desc.is_some() { break; }
        for el in doc.select(&sel_desc) {
            if desc.is_some() { break; }
            desc = Some(String::from(el.inner_html().trim()));
        }
    }

    // a macro to make checking attributes on our image elements less verbose
    macro_rules! check_attr {
        ($elv:expr, $attr:expr) => {
            match $elv.attr($attr) {
                Some(x) => {
                    img = Some(String::from(x));
                    break;
                }
                None => {}
            }
        }
    }
    for sel_img in selector_img {
        if img.is_some() { break; }
        for el in doc.select(&sel_img) {
            if img.is_some() { break; }
            let elv = el.value();
            check_attr!(elv, "src");
            check_attr!(elv, "content");
        }
    }

    Ok(ClipResult::new(title, desc, img))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_stuff() {
        let res = clip(&String::from("https://www.amazon.com/Avoid-Huge-Ships-John-Trimmer/dp/0870334336/ref=pd_lpo_sbs_241_img_2?_encoding=UTF8&psc=1&refRID=SZKJN64CTAYQ44WPNN09"), &vec![]).unwrap();
        assert_eq!(res.title, Some(String::from("How to Avoid Huge Ships: John W. Trimmer: 9780870334337: Amazon.com: Books")));
        assert_eq!(res.description, Some(String::from("Book by Trimmer, John W.")));
        //assert_eq!(res.image_url, Some(String::from("https://images-na.ssl-images-amazon.com/images/I/714PH4X5FRL._SY344_BO1,204,203,200_.gif")));

        let res = clip(&String::from("https://www.youtube.com/watch?v=1KfaQ6pmv18"), &vec![]).unwrap();
        assert_eq!(res.title, Some(String::from("King Gizzard & The Lizard Wizard- I’m In Your Mind Fuzz full album")));
        assert_eq!(res.description, Some(String::from("1.I\'m In Your Mind ")));
        assert_eq!(res.image_url, Some(String::from("https://img.youtube.com/vi/1KfaQ6pmv18/hqdefault.jpg")));
    }
}
