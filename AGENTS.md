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
