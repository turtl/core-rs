use ::error::{TResult, TError};
use ::crypto;
use ::models::protected::Protected;

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

fn use_code(username: &String, password: &String) -> TResult<()> {
    let mut user = User::blank();
    user.set("logged_in", &true).unwrap();
    user.set("changing_password", &false).unwrap();
    let key = try_t!(crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000));
    let key2 = try_t!(crypto::random_key());
    let auth = try_t!(crypto::encrypt_v0(&key, &try_t!(crypto::random_iv()), &String::from("message")));
    user.auth = auth;
    let auth2 = try_t!(crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), try_t!(crypto::CryptoOp::new("aes", "gcm"))));
    let test = try_t!(crypto::decrypt(&key, &auth2.clone()));
    println!("debug stuff: {:?}", (user.stringify_trusted(), auth2, test));
    Ok(())
}

pub fn login(username: String, password: String) -> TResult<()> {
    use_code(&username, &password).unwrap();
    println!("logged in! {}/{}", username, password);
    Ok(())
}

