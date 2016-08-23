//! The THunk defines a function of one generic argument that can be sent across
//! threads inside of a box.

/// Creates a way to call a Box<FnOnce> basically
pub trait Thunk<T: ?Sized>: Send + 'static {
    fn call_box(self: Box<Self>, T);
}
impl<T, F: FnOnce(T) + Send + 'static> Thunk<T> for F {
    fn call_box(self: Box<Self>, arg1: T) {
        (*self)(arg1);
    }
}

