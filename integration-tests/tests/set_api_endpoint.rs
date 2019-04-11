include!("../src/util.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_api_endpoint() {
        let handle = init();
        dispatch_ass(json!(["app:api:set-config", {"endpoint": "http://api.turtl.dev:8181"}]));
        end(handle);
    }
}

