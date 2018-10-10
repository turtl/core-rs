# Turtl Migrate

Migrates profile from 0.6.x to 0.7.x by connecting to an old server, downloading
the profile, decrypting it, and re-encrypting it and pushing to the new server.

Some data is not preserved when migrating:

- Sharing data. While it would have been possible to preserve sharing data, it
would have pushed out the release of the library by quite a long time, and so
it was decided that you can just re-create your shares after migrating. Sorry
=[.
- Notes in multiple boards. Notes in 0.7.x can only belong to one board. There
is some good reasoning behind this: allowing multiple boards complicates logic
and especially complicates interfaces. It's a hard feature to maintain, and I
do not care to do it anymore. Therefore when you migrate, the notes will be added
to the *first board* in their board list.
- Nested boards. Another seldom-used feature that is difficult to maintain and
causes a lot of complexity in the UI. Boards that are nested will be preserved,
but will be siblings to the board that used to be their parent (but with the
name changed to signify it used to be a nested/child board).

