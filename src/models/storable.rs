//! Defines a model which can be stored. Separate from Protected because we
//! sometimes need to define them separately.

pub trait Storable {
    /// Static method for grabbing a model's table
    fn tablename() -> &'static str;

    /// Given a model, grab the table it interacts with
    fn table(&self) -> &'static str {
        Self::tablename()
    }
}

#[macro_export]
macro_rules! make_storable {
    ($ty:ty, $tbl:expr) => {
        impl ::models::storable::Storable for $ty {
            fn tablename() -> &'static str {
                $tbl
            }
        }
    }
}

