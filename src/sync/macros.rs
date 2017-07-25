#[macro_export]
macro_rules! with_db {
    ($dbvar:ident, $dbobj:expr, $errprefix:expr, $( $rest:tt )*) => {
        {
            // TODO: gensym anyone?
            let mut db_guard__ = $dbobj.write().unwrap();
            match db_guard__.as_mut() {
                Some($dbvar) => {
                    $( $rest )*
                }
                None => {
                    return Err(::error::TError::MissingField(format!("{} -- `{}` is None", $errprefix, stringify!($dbobj))));
                }
            }
        }
    }
}

