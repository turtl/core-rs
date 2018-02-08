# Turtl core-rs
<a href="https://circleci.com/gh/turtl/core-rs"><img src="https://circleci.com/gh/turtl/core-rs.svg?style=shield&circle-token=:circle-token"></a>

This is the Rust core for Turtl. It houses the logic for Turtl's main client
operations and is meant to be embedded as a shared/static library that is
standard across all platforms. The idea is, if it *can* go in the core, it
*should* go in the core. Pretty much everything except UI goes here:

- User join/login/deletion
- Talking to the server/syncing data
- Encryption/Decryption of data
- In-memory storage of profile data
- Permissions checking
- Searching of notes
- Sharing/Collaboration handling
- Local storage
- Bookmark handling

When building a UI (Android/iOS/Desktop/etc etc) you should have to worry about
two things: loading/talking to the core and building the interface around the
core. All logic (syncing/crypto/storage) lives in the core.

Although the core project is posted, the new server it talks to (NodeJS/Postgres)
is not yet public (yes, a fond farewell to Lisp). Stay tuned!

This project is unfinished and *alpha* status. I won't be responding to issues
or bug reports on it yet. Use at your own risk.

## Building

```bash
make
```

NOTE: If your system uses OpenSSL 1.1.0, you need to install OpenSSL 1.0.0 and
tell `make` to use it with `OPENSSL_LIB_DIR=/usr/lib/openssl-1.0 OPENSSL_INCLUDE_DIR=/usr/include/openssl-1.0 make`
for example.

NOTE 2: If your system has libsodium version different than 1.0.12 and your build fails, do this:
```
cargo build
wget https://download.libsodium.org/libsodium/releases/libsodium-1.0.12.tar.gz
tar xzf libsodium-1.0.12.tar.gz
cd libsodium-1.0.12
./configure
make
cp -r src/libsodium/.libs/* ../target/debug/deps/
cargo build
```
After every `cargo clean` or `make clean` you should do the last command
