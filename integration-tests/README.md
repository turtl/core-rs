# Integration tests

This crate houses the integration tests for Turtl's core project. The goal of
these tests is to provide 100% coverage for Turtl's [dispatch commands](https://github.com/turtl/core-rs/blob/master/src/dispatch.rs)
and by proxy, make sure the [Turtl server](https://github.com/turtl/server) is
functioning properly as well.

## Usage

[First, set up and run the Turtl server.](https://github.com/turtl/server/blob/master/README.md#running-the-server)

Next, make sure your `turtl/core/config.yaml` file points to your server (the
value you want is `api.endpoint`).

Now build the turtl core main lib:

```sh
cd /path/to/turtl/core
make release
```

Now you can build/run the integration tests:

```sh
cd integration-tests/
make test
```

Good job.

