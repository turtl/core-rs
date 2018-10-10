# Dumpy

An object store. Kind of, sort of mimics indexeddb in that you can dump any
old JSON-serializable object into it and pull it out using various indexes. It's
meant be the backing storage for the (encrypted) object of the Turtl core-rs
project.

It builds on top of SQLite.

The internal representation of the data is somewhat resilient to schema changes
but also somewhat inefficient. All objects, no matter what "table" they're in,
are stored in the same SQLite table. There is a separate table that stores
indexes and is used to look up object IDs from the storage table.

There is also a third table type, a key-value store, which actually more closely
mimics Javascript's localStorage (can you tell Turtl used to be a browser app
yet?).

