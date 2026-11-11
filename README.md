# Banker

A Mac app that shows what was **deleted** or is **damaged** on any device you
plug in: a drive, an SD card, an iPhone or an Android phone. One press backs
the phone up to the Mac (drives are read in place), then lays out everything
deleted or damaged on a timeline you can zoom from years down to the minute.
You can search it, filter it by app, and save it to the Mac sorted by app and
day.

Banker lists nothing else. Healthy files and live messages are read only to
tell them apart from deleted ones, and are never shown.

## What it finds

| Where | What |
| --- | --- |
| Trash folders | Files in `.Trashes`, `.Trash`, `$RECYCLE.BIN`, Android's `.trashed-*` files, app bins under `Android/data/*/trash`, `LOST.DIR`, `FOUND.000` |
| Photos libraries | Photos and videos in **Recently Deleted**, on iPhones and in Mac `.photoslibrary` folders |
| App databases | Deleted messages, contacts, notes and other rows still in an app's SQLite free space (freeblocks, unallocated gaps, freelist pages) and in its write-ahead log, with the row's own date when it has one. Pictures left in that free space are rebuilt too |
| Any file | Damage that the file's own structure proves: truncated JPEGs, PNGs with failed checksums, videos missing their index (`moov`), zero-filled files, archives with no table of contents, cut-short databases |

Each finding is tied to the app it belongs to (from `Android/data/<package>`,
iPhone backup domains, `~/Library/Containers/<bundle id>`, and folders such
as `WhatsApp/`). Pick an app to see everything deleted from it.

## What each device allows

- **iPhone**: Banker makes a full local backup with libimobiledevice, then
  reads it. That reaches Recently Deleted, deleted rows in every app's
  databases, and damaged files. It cannot reach the phone's raw storage
  (Apple encrypts it), so a photo purged from Recently Deleted is gone.
  Encrypted backups are not supported yet: turn off "Encrypt local backup"
  in Finder first.
- **Android**: Banker copies shared storage with `adb` (turn on USB debugging
  on the phone). That reaches the phone's trash, app bins and damaged files.
  Most apps keep their databases in private storage that `adb` cannot read
  without root, so deleted messages on Android are usually out of reach.
- **Drives and SD cards**: read in place, nothing is written. Trash folders,
  damaged files and app databases are covered. Recovering files from a
  drive's free space is not done yet. SSDs erase freed space on their own
  (TRIM), so there is usually nothing left there anyway.

## Set up a Mac for phones

```sh
brew install libimobiledevice          # iPhones and iPads
brew install android-platform-tools    # Android phones
```

The app says which is missing. Backups go to `~/Banker Backups/`.

To scan the Mac's own Trash or Finder's iPhone backups, allow Banker Full
Disk Access in System Settings > Privacy & Security.

## Develop

```sh
npm install
npm run dev        # the app, with live reload
npm run check      # type check, tests, Rust format, lint and tests
npm run dmg        # build and verify a .dmg (on a Mac)
```

Scan a folder from the terminal, without the app:

```sh
cargo run -p banker-core --example scan -- /Volumes/Card
```

## Layout

```
crates/banker-core/    All the logic. No Tauri, so it builds and tests on Linux.
  sqlite.rs            Read-only SQLite reader and deleted-record recovery.
  integrity.rs         Format checks that prove a file is damaged.
  carve.rs             Rebuilding pictures from free space.
  paths.rs             Trash folders, and which app a path belongs to.
  ios.rs               iPhone backups: Manifest.db, Photos' Recently Deleted.
  scan.rs              Walking a device and collecting findings.
  timeline.rs          Search, filters, and year-to-minute grouping.
  devices.rs           Finding drives and phones; copying phones to the Mac.
  export.rs            Thumbnails, and saving findings by app and day.
  tests/fixtures/      Databases written by real SQLite, with known deletions.
src-tauri/             The native shell: window, commands, bundling.
src/                   The React interface.
  lib/native.ts        Every call from the UI to Rust, in one place.
docs/                  dmg.md (packaging failures and fixes), signing.md.
```

Built from the Mac App Template: CI builds and verifies a `.dmg` on a real
Mac for every push. See [docs/dmg.md](docs/dmg.md) and
[docs/signing.md](docs/signing.md) for packaging and releases.
