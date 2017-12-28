v0.7:
- memory leak??
  - running sock continuously seems to add mem indefinitely
  - things to look at:
    - sync threads/data?
    - in-mem sqlite (search) being disposed of properly?
    - profile being cleared?

- integration tests
  - !! test sync after logout WITHOUT clearing app data (need to test incremental sync) !!
  - sync:pause
  - sync:resume
  - sync:get-pending
  - sync:unfreeze-item
  - sync:delete-item
  - profile:find-notes
  - profile:get-file
  - profile:get-tags
  - profile:sync:model
    - edit a note with a file (without re-uploading file, ie just edit title)
      - does the file still remain?
      - does the sync system break in any way?
    - move space
  - check migrate w/ bad login (should fail)
- premium
- profile
  - calculate size

later:
- document core API
  - dispatch endpoints: expected responses, possible errors
  - ui events that can fire (and associated data)
- upgrade sodiumoxide, re-implement AEAD (ietf) over new version (annoying)
- MsgPack for core <--> ui comm
  - https://github.com/3Hren/msgpack-rust
  - https://github.com/kawanet/msgpack-lite
- type system enforce crypto
  - split protected model types (encrypted (for storage), encrypted (in-mem))
  - storage sysem ONLY accepts encrypted model types
  - UI messaging layer ONLY accepts decrypted model types
  - encrypting and decrypting BOTH consume a model and return the new type
- implement i18n? the number of strings grows as the validation strings are
  moved into the core. right now stubbed out as t! macro in util/i18n.rs
  - thinking core should NOT own i18n, it should be owned by each interface
  - we can port the few translations over from js we need (space/board names,
    validation errors) and leave it at that.
- move Turtl.find_model_key(s) et al to protected model (or wherever appropriate)
  - profile loading
  - messaging
  - key management
- file writing locally: use buffers/locks:
  {
      let mut out = File::new("test.out");
      let mut buf = BufWriter::new(out);
      let mut lock = io::stdout().lock();
      writeln!(lock, "{}", header);
      for line in lines {
          writeln!(lock, "{}", line);
          writeln!(buf, "{}", line);
      }
      writeln!(lock, "{}", footer);
  }   // end scope to unlock stdout and flush/close buf


