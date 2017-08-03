include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_api_endpoint() {
        let handle = init();
        send(r#"["1","app:api:set-endpoint","http://api.turtl.dev:8181"]"#);
        let msg = recv("1");
        assert_eq!(msg, r#"{"e":0,"d":{}}"#);
        end(handle);
    }
}

