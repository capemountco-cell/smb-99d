#!/usr/bin/env bash
# Put the phone tools inside Banker.app, so iPhones and Android phones work on
# a Mac with nothing installed.
#
#   iPhone:  idevice_id, ideviceinfo, idevicepair, idevicebackup2
#            (libimobiledevice), with every library they load
#   Android: adb (Android platform tools)
#
# They land in src-tauri/resources/tools/{bin,lib}, which tauri.conf.json
# bundles as Contents/Resources/tools. The libraries are rewritten to load
# from ../lib next to the tools (dylibbundler), and everything is ad-hoc
# signed. Run on a Mac with Homebrew; CI runs it before building the .dmg.
#
# The tools are built for this Mac's chip. On a universal build an Intel Mac
# falls back to tools installed with Homebrew.
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ "$(uname)" != "Darwin" ]]; then
  echo "bundle-tools: macOS only, skipping." >&2
  exit 0
fi
if ! command -v brew >/dev/null; then
  echo "bundle-tools: Homebrew is needed to fetch the phone tools (https://brew.sh)." >&2
  exit 1
fi

out="src-tauri/resources/tools"
rm -rf "$out/bin" "$out/lib"
mkdir -p "$out/bin" "$out/lib"

brew list libimobiledevice >/dev/null 2>&1 || brew install libimobiledevice
brew list dylibbundler >/dev/null 2>&1 || brew install dylibbundler
command -v adb >/dev/null || brew install --cask android-platform-tools

prefix="$(brew --prefix)"
ios_tools=(idevice_id ideviceinfo idevicepair idevicebackup2)
for t in "${ios_tools[@]}"; do
  cp -L "$prefix/bin/$t" "$out/bin/$t"
  chmod 755 "$out/bin/$t"
done
# adb links only system libraries, so it is copied as it is.
cp -L "$(command -v adb)" "$out/bin/adb"
chmod 755 "$out/bin/adb"

fix=()
for t in "${ios_tools[@]}"; do fix+=(-x "$out/bin/$t"); done
dylibbundler -of -b -cd -d "$out/lib" -p @executable_path/../lib/ "${fix[@]}"

# Ad-hoc sign every binary and library after their load paths changed.
# No hardened runtime: they run as Banker's helpers, not as apps.
for f in "$out"/bin/* "$out"/lib/*; do
  codesign --force --sign - "$f"
done

# Prove they start from where they will live.
"$out/bin/idevice_id" -l >/dev/null
"$out/bin/adb" version >/dev/null
echo "bundle-tools: $(ls "$out/bin" | wc -l | tr -d ' ') tools and $(ls "$out/lib" | wc -l | tr -d ' ') libraries in $out"
