# Clouseau

Ah yes, the old "dump everything into SQLite so you can run full-text searches
against it" ploy. Yes, heh heh, very clever indeed!

Basically, Clouseau is an *in-memory* search engine build on top of SQLite. The
in-memory portion is important because it powers the search for Turtl's
core-rs project, which means we don't want the indexed data to be persisted in
any way (due to privacy restrictions).  You have to re-build your index on each
use.

