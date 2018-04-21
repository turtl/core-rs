# cwrap

*(Pronounced CRAP)*

This is a Rust crate that wraps the Turtl core C API. The purpose is to make it
easy for Rust libs to load and run the core without having to implement the
bindings and wrapping themselves.

This is used in the integration tests, the `sock` crate, and the `client` crate.

For the curious, the Turtl C API exposed by the core is [documented here](https://github.com/turtl/core-rs/blob/master/include/turtl_core.h)
and [implemented here](https://github.com/turtl/core-rs/blob/master/src/lib.rs).

