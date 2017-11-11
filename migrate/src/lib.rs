//! A crate/lib for migrating a v0.6.x profile to v0.7.
//!
//! A lot of junk is just kind of dumped inot this main lib. I don't care too
//! much for a well-oiled machine here, it just needs to work. This code is
//! somewhat throwaway.

extern crate config;
extern crate crypto as rust_crypto;
extern crate fern;
extern crate gcrypt;
extern crate hyper;
extern crate jedi;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate quick_error;
extern crate rustc_serialize as serialize;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

#[macro_use]
pub mod error;
mod api;
mod crypto;
mod user;
mod util;

use ::std::io::{Read, Write};
use ::api::{Api, ApiReq};
use ::error::{MError, MResult};
use ::jedi::Value;
pub use crypto::Key;
use ::std::path::PathBuf;
use ::std::fs;
use ::std::collections::HashMap;

/// A shitty placeholder for a sync record
#[derive(Deserialize, Debug)]
struct SyncRecord {
    #[serde(rename = "type")]
    pub ty: String,
    pub data: Option<Value>,
}

/// Holds filedata.
#[derive(Debug, Default)]
pub struct File {
    note_id: String,
    data: Option<Vec<u8>>,
    path: Option<PathBuf>,
}

/// Holds the result of a profile migration.
#[derive(Default, Debug)]
pub struct MigrateResult {
    pub boards: Vec<Value>,
    pub notes: Vec<Value>,
}

/// Holds an encrypted v6 profile
#[derive(Default, Debug)]
pub struct Profile {
    keychain: Vec<Value>,
    boards: Vec<Value>,
    notes: Vec<Value>,
    files: Vec<File>,
}

fn download_file(url: &String, auth: &String) -> MResult<Vec<u8>> {
    let mut headers = hyper::header::Headers::new();
    let client = hyper::Client::new();
    let api_endpoint = config::get::<String>(&["api", "v6", "endpoint"])?;
    if url.contains(api_endpoint.as_str()) {
        let auth_str = String::from("user:") + &auth;
        let base_auth = crypto::to_base64(&Vec::from(auth_str.as_bytes()))?;
        let auth_header = String::from("Basic ") + &base_auth;
        headers.set_raw("Authorization", vec![Vec::from(auth_header.as_bytes())]);
    }
    client
        .request(hyper::method::Method::Get, url.as_str())
        .headers(headers)
        .send()
        .map_err(|e| {
            match e {
                hyper::Error::Io(err) => MError::Io(err),
                _ => tomerr!(e),
            }
        })
        .and_then(|mut res| {
            let mut out = Vec::new();
            res.read_to_end(&mut out)?;
            if !res.status.is_success() {
                let errmsg = String::from_utf8(out)?;
                return Err(MError::Api(res.status, errmsg));
            }
            Ok(out)
        })
}

fn save_file(note_id: &String, contents: Vec<u8>) -> MResult<PathBuf> {
    let mut filepath = PathBuf::from(util::file_folder()?);
    filepath.push(note_id.clone());
    let mut fs_file = fs::File::create(&filepath)?;
    fs_file.write_all(contents.as_slice())?;
    Ok(filepath)
}

fn load_file(note_id: &String) -> MResult<Vec<u8>> {
    let mut filepath = PathBuf::from(util::file_folder()?);
    filepath.push(note_id.clone());
    let mut file = fs::File::open(filepath)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}

fn get_profile<F>(user_id: &String, auth: &String, evfn: &mut F) -> MResult<Profile>
    where F: FnMut(&str, &Value)
{
    let mut api = Api::new();
    api.set_auth(auth.clone())?;
    #[derive(Deserialize, Debug)]
    struct SyncResponse {
        #[serde(default)]
        records: Vec<SyncRecord>,
    }
    let syncdata: SyncResponse = api.get("/sync/full", ApiReq::new().timeout(120))?;
    let SyncResponse { records } = syncdata;
    evfn("profile-download", &Value::Null);
    let mut profile = Profile::default();
    let mut files: Vec<String> = Vec::new();
    let mut num_items = 0;
    for rec in records {
        let SyncRecord { ty, data } = rec;
        if data.is_none() { continue; }
        let data = data.unwrap();
        let rec_user_id: String = match jedi::get(&["user_id"], &data) {
            Ok(x) => x,
            Err(_) => {
                evfn("error", &json!({
                    "msg": format!("missing user_id field for {}", ty),
                    "type": "missing_data",
                }));
                continue;
            }
        };
        // we only want to include records that belong to us
        if &rec_user_id != user_id { continue; }

        match ty.as_ref() {
            "keychain" => {
                profile.keychain.push(data);
            }
            "board" => {
                profile.boards.push(data);
                num_items += 1;
            }
            "note" => {
                // if we have a file, push the note id onto the list
                match jedi::get::<Value>(&["file"], &data) {
                    Ok(_) => {
                        let id = jedi::get(&["id"], &data)?;
                        files.push(id);
                        num_items += 1;
                    }
                    Err(_) => {}
                }
                num_items += 1;
                profile.notes.push(data);
            }
            _ => {}
        }
    }

    evfn("profile-items", &jedi::to_val(&num_items)?);

    evfn("files-pre-download", &jedi::to_val(&files.len())?);
    let filepath = PathBuf::from(util::file_folder()?);
    util::create_dir(&filepath)?;
    for note_id in files {
        let url = api.get::<String>(format!("/notes/{}/file?disable_redirect=1", note_id).as_str(), ApiReq::new())?;
        evfn("file-pre-download", &json!([note_id, url]));
        match download_file(&url, auth) {
            Ok(filedata) => {
                evfn("file-download", &jedi::to_val(&note_id)?);
                let filepath_new = save_file(&note_id, filedata)?;
                profile.files.push(File {
                    note_id: note_id,
                    data: None,
                    path: Some(filepath_new),
                });
            }
            Err(e) => {
                evfn("error", &json!({
                    "msg": format!("{}", e),
                    "type": "file-download",
                    "note_id": note_id,
                    "url": url,
                }));
            }
        }
    }
    Ok(profile)
}

/// Holds login info.
pub struct Login {
    user_id: String,
    auth: String,
    key: Key,
}

impl Login {
    fn new(user_id: String, auth: String, key: Key) -> Self {
        Login {
            user_id: user_id,
            auth: auth,
            key: key,
        }
    }
}

impl ::std::fmt::Debug for Login {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "MigrationLogin {} (auth/key hidden)", self.user_id)
    }
}

/// Check if an account exists on the old server
pub fn check_login(username: &String, password: &String) -> MResult<Option<Login>> {
    let mut api = Api::new();
    let (key1, auth1) = user::generate_auth(username, password, 1)?;
    api.set_auth(auth1.clone())?;
    match api.post::<String>("/auth", ApiReq::new()) {
        Ok(user_id) => { return Ok(Some(Login::new(user_id, auth1, key1))); }
        Err(_) => {}
    }
    let (key0, auth0) = user::generate_auth(username, password, 0)?;
    api.set_auth(auth0.clone())?;
    match api.post::<String>("/auth", ApiReq::new()) {
        Ok(user_id) => { return Ok(Some(Login::new(user_id, auth0, key0))); }
        Err(_) => {}
    }
    Ok(None)
}

/// Migrate a v6 account to a v7 account. We do this by creating sync items
pub fn migrate<F>(v6_login: Login, mut evfn: F) -> MResult<MigrateResult>
    where F: FnMut(&str, &Value)
{
    let profile = get_profile(&v6_login.user_id, &v6_login.auth, &mut evfn)?;
    let decrypted = decrypt_profile(&v6_login.key, profile, &mut evfn)?;

    let mut result = MigrateResult::default();
    result.boards = decrypted.boards.iter().map(|x| x.clone()).collect::<Vec<_>>();
    result.notes = decrypted.notes.iter().map(|x| x.clone()).collect::<Vec<_>>();
    Ok(result)
}

fn detect_old_format(data: &String) -> MResult<Vec<u8>> {
    if data.contains(":i") {
        Ok(Vec::from(data.as_bytes()))
    } else {
        Ok(crypto::from_base64(data)?)
    }
}

fn decrypt_val(key: &Key, val: &Value) -> MResult<Value> {
    let body_base64: String = jedi::get(&["body"], val)?;
    let body = detect_old_format(&body_base64)?;
    let dec: Vec<u8> = crypto::decrypt(key, &body)?;
    let json: String = String::from_utf8(dec)?;
    Ok(jedi::parse(&json)?)
}

fn find_key(keychain: &Vec<Value>, keysearch: &HashMap<String, Key>, val: &Value) -> MResult<Key> {
    let item_id: String = jedi::get(&["id"], val)?;
    // check the keychain
    for keyentry in keychain {
        let kid = match jedi::get::<String>(&["item_id"], keyentry) {
            Ok(x) => x,
            // fuck it
            Err(_) => continue,
            // yes! fuck it! that's your answer to everything!
        };
        if item_id == kid {
            let k: Key = match jedi::get(&["k"], keyentry) {
                Ok(x) => x,
                Err(_) => continue,
            };
            return Ok(k);
        }
    }

    fn decrypt_key(decrypting_key: &Key, encrypted_key: &String) -> MResult<Key> {
        let raw = detect_old_format(encrypted_key)?;
        let decrypted = crypto::decrypt(decrypting_key, &raw)?;
        Ok(Key::new(decrypted))
    }

    // grab item.keys, loop over it, and check keys hash
    let keys: Vec<HashMap<String, String>> = jedi::get_opt(&["keys"], val).unwrap_or(Vec::new());
    for key in keys {
        let mut encrypted_key = None;
        let mut item_id = None;
        for (k, v) in key {
            if k == "k" {
                encrypted_key = Some(v);
            } else {
                item_id = Some(v);
            }
        }
        if encrypted_key.is_none() || item_id.is_none() { continue; }
        let item_id = item_id.unwrap();
        let encrypted_key = encrypted_key.unwrap();

        let item_key = match keysearch.get(&item_id) {
            Some(k) => k.clone(),
            None => continue,
        };
        match decrypt_key(&item_key, &encrypted_key) {
            Ok(k) => return Ok(k),
            Err(_) => {
            }
        }
    }
    Err(MError::NotFound(format!("key not found for {}", item_id)))
}

fn decrypt_profile<F>(user_key: &Key, profile: Profile, evfn: &mut F) -> MResult<Profile>
    where F: FnMut(&str, &Value)
{
    evfn("decrypt-start", &Value::Null);
    let mut profiled = Profile::default();
    for keychain in &profile.keychain {
        let dec = match decrypt_val(user_key, keychain) {
            Ok(x) => x,
            Err(e) => {
                let keychain_id: Option<String> = jedi::get_opt(&["id"], keychain);
                evfn("error", &json!({
                    "msg": format!("{}", e),
                    "type": "decrypt",
                    "subtype": "keychain",
                    "item_id": keychain_id.unwrap_or(String::from("<no id>")),
                }));
                continue;
            }
        };
        profiled.keychain.push(deep_merge(&mut keychain.clone(), &dec)?);
    }

    let mut keysearch: HashMap<String, Key> = HashMap::new();
    // boards can be nested, which means we must do a first pass to grab board
    // keys, then do another pass to use those keys + keychain to decrypt all
    // the boards. if that sounds convoluted, remember i got ~3.5 hours of sleep
    // and my shitty flight is delayed because of smoke from fires in the north
    // bay. in fact, now that i have you here, i'd like to take this opportunity
    // to talk about some changes i would make the the US government if i were,
    // hypothetically, given dictatorial powers. i call this list "in my country
    // there is problem." here goes:
    //
    //   - ban crossovers. get a car or an SUV. not a shitty frankencar.
    //   - mandate ranked choice/STV voting for all state/federal
    //     representatives
    //   - dissolve electoral college
    //   - mandate ranked choice/STV for all future presidential elections
    //   - roll back citizens united, dissolve super PACs.
    //   - single payer health care, if nothing else because it's cheaper than
    //     the stupid half-socialized/forced-market-based health care we have
    //     now. seriously, it's fucking stupid. wake up, sheeple.
    //   - dissolve at&t and comcast, socialize all internet/communication
    //     infrastructure. build public fiber and public LTE towers across the
    //     entire nation, rent the infrastructure to local companies who wish
    //     to offer competing internet services on the public infrastructure.
    //     in other words, internet access will operate on true free-market
    //     principles as opposed to state-protected monopolies.
    //   - divert resources from military to schools. make college tuition, if
    //     not free, very cheap. clamp down on college expansion caused by
    //     excess of student loans. mandatory $50 donation to charity for anyone
    //     who says "free isn't free" WRT to education/health care. no, free is
    //     expensive as fuck, but you know what's more expensive? a generation
    //     of complete fucking morons who have no higher education running a
    //     country with a large military and nuclear arsenal. not just the rich
    //     deserve an education if we're to succeed as a society. quit defending
    //     billionaires, you dumb twats. you will never be that rich. you will
    //     never even come close. this is coming from a guy who didn't go to
    //     college at all.
    //   - all police who carry guns must also wear body cams. no excuses.
    //     police are charged/judged with the same severity as a normal citizen
    //     when dealing with wrongful deaths.
    //   - end private prisons as an instutition. really? an organization that
    //     makes money if people are in prison?? who the fuck thought that was a
    //     good idea? also, focus on reform, not punishment in our PUBLIC
    //     prisons. end the drug war. legalize marijuana. keep other drugs on
    //     schedule, but divert resources to rehab, not prison. in that vein, no
    //     prison for non-violent offenses. yes, rape will be classified as a
    //     violent offense, no matter what state. federal mandate that having
    //     sex with a child is rape regardless of whether or not you marry the
    //     child afterwards. each county must send statistics on who was
    //     sentenced to prison, the term, the crime, and the convict's age,
    //     race, sex/gender, and religion (if any). the idea being that
    //     transparency will make discriminatory sentencing harder to hide.
    //   - mandatory $50 donation to charity any time anyone speaks of lizard
    //     people, chemtrails, or how flooding is caused by homosexuals
    //   - create new corporate entity type, "co-op," and give it extreme tax
    //     benefits, a slow-but-steady move to market socialism. this entity is
    //     optional, but the benefits to the owners (aka all employees) would
    //     make it simply irresistable.
    //   - dissolve marriage as a state-supported instutition. all benefits will
    //     be moved to civil unions, and marriage is reserved for whatever
    //     religion you happen to be a part of that week. if you don't want to
    //     recognize someone else's marriage, great, good for you, but the state
    //     honors all civil unions between any two people.
    //   - dissolve dictatorship once the above is complete
    //
    // anyway, first pass, just find board keys.
    for board in &profile.boards {
        let board_id = match jedi::get(&["id"], board) {
            Ok(x) => x,
            Err(e) => {
                evfn("error", &json!({
                    "msg": format!("board id not present: {}", e),
                    "type": "decrypt",
                    "subtype": "board",
                    "item_id": "<no id>",
                }));
                continue;
            }
        };
        match find_key(&profiled.keychain, &keysearch, board) {
            Ok(x) => {
                keysearch.insert(board_id, x);
            }
            Err(_) => {
                // since this is the first pass, we don't necessary have a
                // problem yet. don't send an error.
                continue;
            }
        }
    }

    // second pass for boards, find keys + decrypt
    for board in &profile.boards {
        let board_id: String = match jedi::get(&["id"], board) {
            Ok(x) => x,
            Err(_) => {
                // we already sent an error for this board if it doesn't have
                // an id, no sense in duplicating errors
                continue;
            }
        };
        match find_key(&profiled.keychain, &keysearch, board) {
            Ok(x) => {
                keysearch.insert(board_id.clone(), x.clone());
                let dec = match decrypt_val(&x, board) {
                    Ok(x) => x,
                    Err(e) => {
                        evfn("error", &json!({
                            "msg": format!("{}", e),
                            "type": "decrypt",
                            "subtype": "board",
                            "item_id": board_id,
                        }));
                        continue;
                    }
                };
                evfn("decrypt-item", &Value::String(String::from("board")));
                profiled.boards.push(deep_merge(&mut board.clone(), &dec)?);
            }
            Err(e) => {
                evfn("error", &json!({
                    "msg": format!("can't find board key: {}", e),
                    "type": "decrypt",
                    "subtype": "board",
                    "item_id": board_id,
                }));
                continue;
            }
        }
    }

    for note in &profile.notes {
        let note_id: String = match jedi::get(&["id"], note) {
            Ok(x) => x,
            Err(e) => {
                evfn("error", &json!({
                    "msg": format!("note id not present: {}", e),
                    "type": "decrypt",
                    "subtype": "note",
                    "item_id": "<no id>",
                }));
                continue;
            }
        };
        match find_key(&profiled.keychain, &keysearch, note) {
            Ok(note_key) => {
                keysearch.insert(note_id.clone(), note_key.clone());
                let dec = match decrypt_val(&note_key, note) {
                    Ok(x) => x,
                    Err(e) => {
                        evfn("error", &json!({
                            "msg": format!("{}", e),
                            "type": "decrypt",
                            "subtype": "note",
                            "item_id": note_id,
                        }));
                        continue;
                    }
                };
                evfn("decrypt-item", &Value::String(String::from("note")));
                fn get_file(note_id: &String, note_key: &Key, notedata: &Value) -> Option<String> {
                    if jedi::get_opt::<Value>(&["file"], &notedata).is_none() { return None; }
                    let encdata = match load_file(note_id) {
                        Ok(x) => x,
                        Err(_) => { return None; }
                    };
                    let filedec = match crypto::decrypt(note_key, &encdata) {
                        Ok(x) => x,
                        Err(_) => { return None; }
                    };
                    let filedec_base64 = match crypto::to_base64(&filedec) {
                        Ok(x) => x,
                        Err(_) => { return None; }
                    };
                    Some(filedec_base64)
                }
                let mut merged_note = deep_merge(&mut note.clone(), &dec)?;
                if let Some(filebase64) = get_file(&note_id, &note_key, note) {
                    evfn("decrypt-item", &Value::String(String::from("file")));
                    match jedi::set(&["file", "filedata"], &mut merged_note, &json!({"data": filebase64})) {
                        Ok(_) => {},
                        Err(e) => {
                            evfn("error", &json!({
                                "msg": format!("{}", e),
                                "type": "decrypt",
                                "subtype": "note-file",
                                "item_id": note_id,
                            }));
                        }
                    }
                }
                profiled.notes.push(merged_note);
            }
            Err(e) => {
                evfn("error", &json!({
                    "msg": format!("can't find note key: {}", e),
                    "type": "decrypt",
                    "subtype": "note",
                    "item_id": note_id,
                }));
                continue;
            }
        }
    }
    Ok(profiled)
}

fn deep_merge(val1: &mut Value, val2: &Value) -> MResult<Value> {
    if !val1.is_object() || !val2.is_object() {
        return Err(MError::BadValue(String::from("deep_merge() -- bad objects passed")));
    }

    {
        let obj1 = val1.as_object_mut().unwrap();
        let obj2 = val2.as_object().unwrap();
        for (key, val) in obj2 {
            if val.is_object() {
                let merged_val = {
                    let mut obj1_val = obj1.entry(key.clone()).or_insert(json!({}));
                    if !obj1_val.is_null() && !obj1_val.is_object() {
                        return Err(MError::BadValue(String::from("deep_merge() -- trying to merge an object into a non-object")));
                    }
                    deep_merge(&mut obj1_val, &val)?
                };
                obj1.insert(key.clone(), merged_val);
            } else {
                obj1.insert(key.clone(), val.clone());
            }
        }
    }
    Ok(val1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::jedi;

    #[test]
    fn deep_merge_test() {
        let mut obj1 = json!({
            "name": "jerry",
            "location": {
                "city": {
                    "name": "santa cruz",
                    "latlon": "123,42",
                },
            }
        });
        let obj2 = json!({
            "name": "sandra",
            "location": {
                "state": {
                    "id": "CA",
                },
                "city": {
                    "name": "santa cruz huaghh",
                },
            }
        });
        let merged = deep_merge(&mut obj1, &obj2).unwrap();

        assert_eq!(jedi::get::<String>(&["name"], &merged).unwrap(), "sandra");
        assert_eq!(jedi::get::<String>(&["location", "state", "id"], &merged).unwrap(), "CA");
        assert_eq!(jedi::get::<String>(&["location", "city", "name"], &merged).unwrap(), "santa cruz huaghh");
        assert_eq!(jedi::get::<String>(&["location", "city", "latlon"], &merged).unwrap(), "123,42");
    }

    #[test]
    #[should_panic]
    fn deep_merge_test_panic() {
        let mut obj1 = json!({
            "name": "jerry",
        });
        let obj2 = json!({
            "name": {
                "first": "harold",
                "last": "barreled",
            },
        });
        deep_merge(&mut obj1, &obj2).unwrap();
    }
}

