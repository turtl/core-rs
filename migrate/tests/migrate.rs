extern crate migrate;
extern crate config;

#[cfg(test)]
mod tests {
    use ::config;
    use ::migrate;
    use ::std::env;

    #[test]
    fn migrates_lol() {
        if env::var("TURTL_CONFIG_FILE").is_err() {
            env::set_var("TURTL_CONFIG_FILE", "../config.yaml");
        }

        let username = config::get::<String>(&["integration_tests", "v6_login", "username"]).unwrap();
        let password = config::get::<String>(&["integration_tests", "v6_login", "password"]).unwrap();
        let login = migrate::check_login(&username, &password).unwrap();
        assert_eq!(login.is_some(), true);
        let migration = migrate::migrate(login.unwrap()).unwrap();
    }
}

