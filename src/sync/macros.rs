#[macro_export]
macro_rules! with_db {
    ($dbvar:ident, $dbobj:expr, $( $rest:tt )*) => {
        {
            // TODO: gensym anyone?
            let mut db_guard__ = $dbobj.write().unwrap();
            match db_guard__.as_mut() {
                Some($dbvar) => {
                    $( $rest )*
                }
                None => {
                    return TErr!(::error::TError::MissingField(format!("{}", stringify!($dbobj))));
                }
            }
        }
    }
}

