//! Defines a model which can be stored. Separate from Protected because we
//! sometimes need to define them separately.

pub trait Storable {
    fn table(&self) -> &'static str;
}

#[macro_export]
macro_rules! make_storable {
    ($ty:ty, $tbl:expr) => {
        impl ::models::storable::Storable for $ty {
            fn table(&self) -> &'static str {
                $tbl
            }
        }
    }
}

