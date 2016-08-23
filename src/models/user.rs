use ::error::{TResult, TFutureResult, TError};
use ::crypto;
use ::api::Api;
use ::models::protected::Protected;
use ::futures::{self, Future};
use ::turtl::TurtlWrap;

protected!{
    pub struct User {
        ( storage: i64 ),
        ( settings: ::util::json::Value ),
        (
            auth: String
            //logged_in: bool,
            //changing_password: bool
        )
    }
}

impl User {
}

fn generate_key(username: &String, password: &String, version: u16) -> TResult<Vec<u8>> {
    let key: Vec<u8> = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            // and laughter too
            salt.push_str(":a_pinch_of_salt");
            let res_key = crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400)
                .map_err(|e| TError::CryptoError(e));
            try!(res_key)
        },
        1 => {
            Vec::new()
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(key)
}

fn generate_auth(username: String, password: String, version: u16) -> TResult<String> {
    let key = try!(generate_key(&username, &password, 1));
    let auth = match version {
        0 => {
            String::new()
        },
        1 => {
            String::new()
        },
        _ => return Err(TError::NotImplemented),
    };
    Ok(auth + "test")
}

/// A worthless function that doesn't do much of anything except keeps the
/// compiler from bitching about all my unused crypto code.
fn use_code(username: &String, password: &String) -> TResult<()> {
    let mut user = User::blank();
    user.set("logged_in", &true).unwrap();
    user.set("changing_password", &false).unwrap();
    let key = try_t!(crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000));
    let key2 = try_t!(crypto::random_key());
    let auth = try_t!(crypto::encrypt_v0(&key, &try_t!(crypto::random_iv()), &String::from("message")));
    user.auth = auth;
    let auth2 = try_t!(crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), try_t!(crypto::CryptoOp::new("aes", "gcm"))));
    let test = String::from_utf8(try_t!(crypto::decrypt(&key2, &auth2.clone())));
    println!("debug stuff: {:?}", (user.stringify_trusted(), auth2, test));
    Ok(())
}

impl User {
    pub fn login(turtl: TurtlWrap, username: String, password: String) -> TFutureResult<()> {
        let ref work = turtl.read().unwrap().work;
        let turtlc = turtl.clone();
        println!("- user: gen auth");
        work.run(move || generate_auth(username, password, 1))
            .and_then(move |auth: String| {
                println!("- user: auth: {}", auth);
                let ref work = turtlc.read().unwrap().work;
                work.run(|| use_code(&String::from("ass"), &String::from("butt")))
            })
            .map(|_| println!("- user: used code"))
            .boxed()
    }
}

