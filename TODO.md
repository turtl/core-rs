v0.7:
- invites
  - integration tests!
  - deserializing user after logging in as testdata@turtlapp.com throws a crypto
    error. does this happen on login_sync_logout?
- model
  - make id_or_else(), implement everywhere
- update prot.(de)serialize() to consume model and return model so calls to
  work() can skip the dumbass cloning
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
- convert Turtl.db to use Mutex instead of RwLock
- move Turtl.find_model_key(s) et al to protected model (or wherever
  appropriate)
- rename KEychainEntry.type\_ to ty

