include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;

    use ::std::thread;
    use ::carrier;

    fn end(handle: thread::JoinHandle<()>) {
        send(r#"["4269","app:shutdown"]"#);
        handle.join().unwrap();
        carrier::wipe();
    }

    #[test]
    fn ping_pong() {
        let handle = init();
        send(r#"["0","ping"]"#);
        let msg = recv("0");
        assert_eq!(msg, r#"{"e":0,"d":"pong"}"#);
        end(handle);
    }
}

