v0.7:
- invites
  - edit invite
  - delete invite
  - ^ permissions!
  - NOTE: invite sending/accepting/edit/delete requires connection
  - make sure to save keychain after adding invite space key
  - integraiton tests!
- bookmarker
  - just takes a url (no http server, leave that to wrapper)
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

