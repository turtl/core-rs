include!("../src/util.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::config;

    #[test]
    fn save_login() {
        let handle = init();
        let username: String = config::get(&["integration_tests", "login", "username"]).unwrap();
        let password: String = config::get(&["integration_tests", "login", "password"]).unwrap();

        dispatch_ass(json!(["app:wipe-app-data"]));
        dispatch_ass(json!(["user:login", username, password]));
        wait_on("user:login");
        let saved_login = dispatch_ass(json!(["user:save-login"]));
        let saved_login_id: String = jedi::get(&["user_id"], &saved_login).unwrap();
        let saved_login_key: String = jedi::get(&["key"], &saved_login).unwrap();
        dispatch_ass(json!(["user:logout"]));
        sleep(10);

        dispatch_ass(json!(["user:login-from-saved", saved_login_id, saved_login_key]));
        wait_on("user:login");
        dispatch_ass(json!(["user:logout"]));
        dispatch_ass(json!(["app:wipe-app-data"]));
        end(handle);
    }
}

