v0.7:
- syncing:
  - notify core of UI data change
    - reindex on note change
  - track sync item failures via `freeze_sync_record`
  - sync errors are now embedded in each failed sync item, no more passing errors
    to `notify_sync_failure`
  - file syncing
    - outgoing file uploads
      - can we do this without queuing? perhaps a stateless query that says
        "here's all the notes w/ files i have, what are their file ids?"
        then compare the ids to what we have locally
    - incoming file downloads
      - store files in filesystem (not sqlite)
      - filenames should be the <note.id>_<note.file.id>.enc
  - send outgoing sync to api
- user
  - change password
- invites
  - copy invite system from js
  - NOTE: invite sending/accepting requires connection
  - make sure to save keychain after adding invite space key
- get sync info (pending, failed)
- send feedback
- implement sync.immediate
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

