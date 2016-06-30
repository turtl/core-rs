use ::error::{TResult};

pub fn login(username: String, password: String) -> TResult<()> {
    println!("logged in! {}/{}", username, password);
    Ok(())
}

