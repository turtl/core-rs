use ::error::{TResult, TError};
use ::crypto;
/*
use ::models::protected::Protected;

pub struct User {
    id: String,
    settings: UserSettings,
    storage: u64,
}

impl Protected for User {
    fn public_fields(&self) -> Vec<&str> {
        return vec![
            "id",
            "storage",
        ];
    }

    fn private_fields(&self) -> Vec<&str> {
        return vec![
            "settings"
        ];
    }
}
*/

fn use_code(username: &String, password: &String) -> TResult<()> {
    let key = try_t!(crypto::gen_key(crypto::Hasher::SHA256, password, username.as_bytes(), 100000));
    let key2 = try_t!(crypto::random_key());
    let auth = try_t!(crypto::encrypt_v0(&key, &try_t!(crypto::random_iv()), &String::from("message")));
    let auth2 = try_t!(crypto::encrypt(&key2, Vec::from(String::from("message").as_bytes()), try_t!(crypto::CryptoOp::new("aes", "gcm"))));
    let test = try_t!(crypto::decrypt(&key, &auth2.clone()));
    println!("debug stuff: {:?}", (auth, auth2, test));
    Ok(())
}

pub fn login(username: String, password: String) -> TResult<()> {
    use_code(&username, &password).unwrap();
    println!("logged in! {}/{}", username, password);
    Ok(())
}

