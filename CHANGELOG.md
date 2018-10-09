# v0.1.2

v0.7 has been released and here are some updates we've done for it:

Features:

- convert all use of .unwrap() to .expect() with descriptive errors. while we
obviously avoid using either for anything serious, sometimes there's not much
else you can do, and an expect() at least gives more information
- the messaging system now tries more resilient methods of character decoding.
the idea here is to make sure (preemtively) that it plays nice with a javascript
UI.
- turtl now locks its data directory so only one instance of the core can run
at once. this fixes some strange issues that happen when multiple turtl
instances run at the same time (using the same database(s))

Fixes:

- fixing some issues in the sync system that were causing weird disconnects.

# v0.1.1

This is a pre-v0.7 maintenance release.

Features: 

- Converting to crate type cdylib, which bundles rust-std and makes it so we
don't have to jump through a bunch of hoops when bundling the app. Also keeps
the compiled binary size down. This required building a (much-needed) core
abstraction (cwrap) that loads the core's chared lib and lets us use it from
Rust (very handy for integration tests, the websocket server, etc).
- New dispatch command (`app:get-config`) to grab core config.
- Migration crate is now much more resilient to file download failures.
- Upgrade to rusqlite 0.13.0.
- Upgrading logging crates (log/fern) to latest versions (0.4.1/0.5.5). The idea
was this would fix a linking problem when cross compiling to Android, but it did
not. Oh well, newer crates are nice!
- Adding JNI export for java apps (android, mainly, but others should be able to
take advantage)
- Adding logging of errors in `turtlc_*` functions, and new function to return
the last error: `turtlc_lasterr` (see [turtl\_core.h](https://github.com/turtl/core-rs/blob/master/include/turtl_core.h))
and also `turtlc_free_err`. adding JNI wrapper around this API.
- Updating `turtlc_recv[_event]` functions to set msglen = 1 if an error occurs.
- Changing the way global config is loaded, so it can be more customized (and
also allowed setting the config file via runtime config via the `config_file`
key).
- can now log to a file, as well as plain old STDOUT. this is going to be more
than helpful once Turtl is out in the wild...i can just have people send me
their log file and pick through it. log file is rotated by size via params set
in the config. very nice/handy.
- ability to set openssl root cert file by config param. useful for android,
which COMPLETELY SCREWS UP THE CERT STORE. yet another hurdle android is tossing
my way. this config param sets the `SSL_CERT_FILE` env var, which thankfully
really does work, provided you also create the file via your frontend in the
android app (ugh).
- removing bundled sodiumoxide (with unmerged aead exports) and swapping in
published 0.0.16 version. this is mainly because the bundled version only ran
against libsodium v1.0.12, but 1.0.12 has a bug in arm64 which makes the pwhash
spit out incorrect keys (breaking turtl logins on android). decided to just go
in and upgrade everything sodium-related, turned out to me much easier than I
thought it'd be.
- Adding interface to save logins...basically, you call it after you've logged
in and it encrypts your login data with a random key and saves it to disk. It
then hands you back the encryption key, and it's the responsbility of the caller
to store the key somewhere safe and recall it when it's time to log in again.
This lowers the barrier to entry for "Remember me" features quite a bit.
- Adding an `app:get-log` endpoint which returns the last N lines of the core
log. Very nice for debugging.

Fixes:

- Always lowercase username (email) for login/join/etc.
- Updating some CircleCI routines (including rust 1.27.1).
- Fixing auth bug when grabbing files from S3 (or any non-turtl-api source).
- Fixing bug where Thredder blindly accepts `0` for # pollers (and also upgraded
num\_cpus crate).
- Fixing bug where if the API responds but returns an HTTP error, we were not
marking sync as disconnected. Now, basically anything other than an HTTP 2xx
will mark us as not syncing.
- Adjusting build to exclude some unneeded libraries.
- Merging [rust-crypto#384](https://github.com/DaGenix/rust-crypto/pull/384) to
fix an android build issue (aarch64).
- Migration data path fix
- Upgrading quick-error crate (1.1.0 -> 1.2.2)
- Adding integration test for key loss test case
- Removing rustc_serialise (deprecated) and replacing it with some specific
crates
- Fixing some timing issues with the WebSocket server
- Centralizing the function that grabs the current storage location for the
core (so we can use it as a building block to grab the core's files/logs/etc).
- Fixing a bug where user.privkey (for decrypting messages/invites) gets lost
after joining. Also, adding an `ensure_keypair` fn to the User model that looks
for a missing keypair and generates one if needed.


# v0.1.0

Built the turtl core project.

This was a rearchitect based off the javascript frontend ([js#v0.7](https://github.com/turtl/js/tree/v0.7))
which accomplished the following:

- Username is now a public field, and must be an email address. Email addresses
must be verified before sharing is enabled. Having usernames be private /
encrypted was a huge source of confusion and anguish for both the maintainers of
Turtl and the users. So we're giving up some anonymity for convenience. That
said, anonymity is *not* a core goal of the Turtl project, and so this change is
in line with the project's mission.
- Added a new top-level type called Spaces. Spaces contain both boards and notes
and are now the only shareable object in Turtl. Spaces act as "silos" for data,
allowing various "modes" you can switch between. For instance, you might have a
"Personal" Space where all your favorite WKUK youtube videos are bookmarked, a
"Work" Space where you and your coworkers can share notes and files with each
other, or a "Private" space where you keep your videos of dogs humping people's
legs (no judgements). Spaces have all the same privacy protections that boards
and notes do (all fields of spaces, including "title," are private except for
the owner/user id, when it was created, and when it was last edited). Spaces
exist to keep various aspects of your life separate from each other.
- Shares now have multiple [permission levels](https://github.com/turtl/lib-permissions),
so you can have read-only members, members who can only add/edit notes, or full
admins who can manage users, invites, boards, notes etc. There is also an
"owner" permission that one lucky user gets (usually the creator of the Space)
who has access to everything, including giving ownership to another person.
- Boards can no longer have child boards. I know, I know. Some of you are
cursing me under your breath. Most of you, however, probably never even used the
feature. This was a fairly data-driven decision, and really did help keep the
app simpler...which means I could actually release this instead of working on it
another six months. That's something, right?
- Notes can now only exist in at most one board. This was another decision I
made to keep things simple. Allowing notes in multiple boards not only confused
many people but it complicated the interface quite a bit, without really
justifying its utility.

Architecture aside, here's what happened in `core-rs`:

- Took large swaths of code from js, converted it to Rust, and in many cases
completely rearchitected it.
- Wrote migration tool for moving from v0.6 to v0.7+. The migration *loses
shares* so you will have to re-share your Spaces once your migrate. The amount
of complication involved with migrating share data between architectures was so
extreme that I couldn't justify the amount of time to build, test, and support
the feature. I'm hoping it won't be too much of a problem for people to redo
their shares. Thankfully, Turtl is marked as a beta so at least I've got that to
point to if people get mad ;).
- Exposed a C API from Rust so the project could be embedded in desktop/mobile
via shared or static libs.
- Wrote extensive unit and integration tests that talk to the [new server](https://github.com/turtl/server)
which make sure the core and the server are both in healthy condition.
- Wrote automated CI builds/releases for linux/windows/osx/android on x32/x64.

I am probably missing some stuff, but if you're SUPER INTERESTED you can rummage
through the commit log. There are no doubt not only insights into the
architecture, but many late-night antisocial comments and jabs. Find them all!

That about covers it! Enjoy!

