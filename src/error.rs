quick_error! {
    #[derive(Debug)]
    pub enum TError {
        Shutdown {
            description("shutting down")
            display("shutting down")
        }
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
        BadValue(str: String) {
            description(str)
            display("bad value: {}", str)
        }
    }
}

pub type TResult<T> = Result<T, TError>;

/// converts non-TError errors to TError. this is a macro because I am sure this
/// is the "wrong" way to do this and once I know a better way, I can hopefully
/// fix it later
#[macro_export]
macro_rules! toterr {
    ($e:expr) => (TError::Msg(format!("{}", $e)))
}

/// try!-esque wrapper around toterr
#[macro_export]
macro_rules! try_t {
    ($e:expr) => (try!($e.map_err(|e| toterr!(e))))
}

