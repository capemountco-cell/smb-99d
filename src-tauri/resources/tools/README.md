# Bundled phone tools

`scripts/bundle-tools.sh` fills `bin/` and `lib/` here with libimobiledevice
(iPhones) and adb (Android) before a release build, and `tauri.conf.json`
bundles this folder into Banker.app as `Contents/Resources/tools`. Banker
looks there first (`BANKER_TOOLS_DIR`, set at startup), then on PATH and in
Homebrew's folders.

`bin/` and `lib/` are build output and are not committed.
