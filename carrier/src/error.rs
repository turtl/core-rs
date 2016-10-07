//! Define our error/result structs

quick_error! {
    #[derive(Debug)]
    /// Dumpy's main error object.
    pub enum CError {
        Msg(str: String) {
            description(str)
            display("error: {}", str)
        }
    }
}

pub type CResult<T> = Result<T, CError>;

