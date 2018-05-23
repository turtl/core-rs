use ::std::error::Error;

quick_error! {
    /// Define a type for cryptography errors.
    #[derive(Debug)]
    pub enum CryptoError {
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("crypto: error: {}", err.description())
        }
        Msg(str: String) {
            description(str)
            display("crypto: error: {}", str)
        }
        Authentication(str: String) {
            description("authentication error")
            display("crypto: authentication error: {}", str)
        }
        BadData(str: String) {
            description("bad data")
            display("crypto: bad data: {}", str)
        }
        OperationFailed(str: String) {
            description("operation failed")
            display("crypto: operation failed: {}", str)
        }
        NotImplemented(str: String) {
            description("not implemented")
            display("crypto: not implemented: {}", str)
        }
    }
}

macro_rules! make_boxed_err {
    ($from:ty) => {
        impl From<$from> for CryptoError {
            fn from(err: $from) -> CryptoError {
                CryptoError::Boxed(Box::new(err))
            }
        }
    }
}
make_boxed_err!(::hex::FromHexError);
make_boxed_err!(::base64::DecodeError);

pub type CResult<T> = Result<T, CryptoError>;

