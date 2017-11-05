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

