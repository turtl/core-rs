/// Counts the idents in a given list
#[macro_export]
macro_rules! count_idents {
    () => { 0usize };
    ( $x:ident ) => { 1usize };
    ( $x:ident, $( $y:ident ),* ) => { 1usize + count_idents!( $($y),* ) };
}


