# Working on this app

This repository started from the Mac App Template. These rules keep it able
to produce a working `.dmg` at all times.

## Rules

**Logic goes in the core crate** (`crates/*-core`), never in Tauri command
handlers or React components. A command handler deserialises, calls one core
function, and returns. The core crate must not depend on Tauri, so its tests
run on Linux.

**Every UI call to Rust goes through `src/lib/native.ts`.** Components never
call `invoke` directly.

**Do not change these without reading docs/dmg.md:**
- `bundle.macOS.signingIdentity: "-"`. Without a signature a downloaded app
  is reported as "damaged".
- `"version": "../package.json"` in `tauri.conf.json`. Set versions only with
  `npm run set-version`.
- The dev port (1420) in both `tauri.conf.json` and `vite.config.ts`.
- `runs-on: macos-latest` in the workflows. Pinned images get retired, and a
  job naming one is cancelled with no logs.

**Permissions start at nothing.** Add Tauri capabilities in
`src-tauri/capabilities/default.json` and entitlements in
`src-tauri/entitlements.plist` one at a time, as features need them, with a
comment saying why.

**Do not add dependencies casually.** Prefer fifty lines of code to a library
for a trivial problem.

## Before pushing

```sh
npm run check
```

On a Mac, before anything that touches `src-tauri/`, packaging or icons:

```sh
npm run dmg
```

CI builds and verifies a `.dmg` on every push, so a packaging break shows up
on the pull request that caused it.

## Banker's own rules

**Only deleted or damaged content is ever shown.** Every `Finding` is
`Status::Deleted` or `Status::Corrupt`. Live files and live rows may be read
to tell them apart (for example `live_text` in `sqlite.rs`), never reported.
A new source of findings needs a test proving it does not surface live data.

**Never write to a device.** Drives are scanned in place, read-only. Phones
are copied to `~/Banker Backups/` first and the copy is scanned. Rebuilt
pictures and thumbnails go to the app's cache folder.

**Recovery is checked against real SQLite.** `crates/banker-core/tests/`
uses databases written by SQLite itself (see `tests/fixtures/README.md`).
Hand-built pages in unit tests are not enough: real page layouts (rebalanced
pages, merged freeblocks, stale cells in the unallocated gap) are where
false positives come from.

**Phone tools are external.** iPhones use libimobiledevice and Android uses
`adb`, found on PATH or in Homebrew's folders (`devices::find_tool`). If one
is missing, the app shows how to install it rather than failing.
