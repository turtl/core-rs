v0.7:
- import/export
  - find out deadlock
- migration crate
  - move old crypto, old user keygen/authgen to migration crate
  - check_account() -- checks old login on old server, signals "valid" or not
  - migrate_account() -- takes older server, old login, new server, new login
    - download data
	- decrypt keychain/boards
	- create a default space "Personal" or some shit
	- move all boards into the new space
	- move all notes into the new space
- integration tests
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
  - feedback:send??
- premium

later:
- document API
  - dispatch endpoints: expected responses, possible errors
  - ui events that can fire (and associated data)
- MsgPack for core <--> client comm
  - https://github.com/3Hren/msgpack-rust
  - https://github.com/kawanet/msgpack-lite
- type system enforce crypto
  - split protected model types (encrypted (for storage), encrypted (in-mem))
  - storage sysem ONLY accepts encrypted model types
  - UI messaging layer ONLY accepts decrypted model types
  - encrypting and decrypting BOTH consume a model and return the new type
- implement i18n? seems the only place using it is the user model. maybe not a
  big deal to just have a few hardcoded english items?
- investigate more stateless way of syncing files?
- move Turtl.find_model_key(s) et al to protected model (or wherever
  appropriate)
  - profile loading
  - messaging
  - key management
- rename KEychainEntry.type\_ to ty
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


