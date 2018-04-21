# Sock

This is a websock wrapper around the turtl core (using `cwrap`). The purpose is
to be able to quickly iterate over a web UI without needing the repackage/bundle
the library onto some platform that can load shared objects.

If you load [turtl/js](https://github.com/turtl/core-rs) in a browser, its first
instinct will be to talk to this crate!

(Don't use this for anything but testing!)

## Usage

First, build the turtl core main lib:

```sh
cd /path/to/turtl/core
make release
```

Then you can build/run sock:
```sh
cd sock/
make run
```

You are now ready to receive Turtls on port `7472`.

