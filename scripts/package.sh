#!/bin/sh
# Build the app bundle and package it, with the README and license files, as
# dist/Mathstral-<version>-alpha-macos-<arch>.zip.
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

sh "$root/scripts/build-app.sh"

app="$root/target/Mathstral.app"
version=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")
arch=$(uname -m)
name="Mathstral-$version-alpha"
zip="$root/dist/$name-macos-$arch.zip"
stage="$root/dist/stage/$name"

rm -rf "$root/dist/stage"
mkdir -p "$stage"
ditto "$app" "$stage/Mathstral.app"
cp README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.md "$stage/"

rm -f "$zip"
# --norsrc --noextattr --noqtn keep AppleDouble "._" entries out of the archive.
ditto -c -k --norsrc --noextattr --noqtn --keepParent "$stage" "$zip"
rm -rf "$root/dist/stage"

printf '%s\n' "$zip"
shasum -a 256 "$zip"
