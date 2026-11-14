# Test databases

Made by real SQLite (Python's `sqlite3`), so the tests see the layouts SQLite
actually produces, not hand-built pages.

- `messages.db`: table `messages (_id INTEGER PRIMARY KEY, key_remote_jid,
  data, timestamp)` with 200 rows, "message number 0" to "message number 199",
  timestamps in milliseconds. Rows with `_id` 17, 42, 99 and every `_id` over
  150 were then deleted (so "message number 16", 41, 98 and 150 to 199 are
  deleted; everything else is live).
- `contacts.db` and `contacts.db-wal`: WAL-mode table `contacts` with Alice
  Smith, Bob Jones and Carol White, checkpointed, then Bob Jones deleted.
  The deletion lives only in the WAL.
- `mac-photos.sqlite`: the two tables of a Mac Photos library that Reco
  reads. `ZASSET` has three assets (ids ending 1, 2 and 3); 1 and 2 are in
  Recently Deleted (`ZTRASHEDSTATE` 1), 3 is not. `ZADDITIONALASSETATTRIBUTES`
  gives their imported names, IMG_6352.JPG, IMG_6353.HEIC and IMG_6354.HEIC.
  `tests/mac_photos.rs` gives asset 1 an original and asset 2 only previews,
  as "Optimize Mac Storage" does.
