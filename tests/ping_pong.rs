include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_pong() {
        let handle = init();
        let pongval = dispatch_ass(json!(["ping"]));
        let msg: String = jedi::from_val(pongval).unwrap();
        assert_eq!(msg, "pong");
        end(handle);
    }
}

