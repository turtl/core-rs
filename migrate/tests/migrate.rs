extern crate migrate;
extern crate config;

#[cfg(test)]
mod tests {
    use ::config;
    use ::migrate;

    fn init() {
        config::load_config(None).unwrap();
    }

    #[test]
    fn migrates_lol() {
        init();
        let username = config::get::<String>(&["integration_tests", "v6_login", "username"]).unwrap();
        let password = config::get::<String>(&["integration_tests", "v6_login", "password"]).unwrap();
        let login = migrate::check_login(&username, &password).unwrap();
        assert_eq!(login.is_some(), true);
        migrate::migrate(login.unwrap(), |ev, args| {
            println!("migrate: event: {} -- {}", ev, args);
        }).unwrap();
    }
}

