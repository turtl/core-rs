v0.7:
- syncing:
  - file syncing
    - incoming file downloads
    - remove file .....file on note delete
    - wipe_data should remove file records
  - profile:load -- return user object =]
  - remove FileDAta::pathbuf_to_string/get_note_id if still not needed after
    implementing incoming file sync
- user
  - change password
  - set space.default = true based on settings
- spaces
  - remove key from keychain on delete
- invites
  - copy invite system from js
  - NOTE: invite sending/accepting requires connection
    - app events?
  - make sure to save keychain after adding invite space key
- bookmarker
- migration crate
  - move old crypto, old user keygen/authgen to migration crate
  - check_account() -- checks old login on old server, signals "valid" or not
  - migrate_account() -- takes older server, old login, new server, new login
    - download data
	- decrypt keychain/boards
	- create a default space "Personal" or some shit
	- move all boards into the new space
	- move all notes into the new space
- premium

later:
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
- implement enum for sync types (instead of "note", "board", etc)
- update dumpy/storage to use <T: DeserializeOwned> instead of passing Value
- investigate more stateless way of syncing files?

