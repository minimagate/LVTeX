#!/bin/sh
# Build the release binary and assemble target/Mathstral.app (ad-hoc signed).
set -eu
cd "$(dirname "$0")/.."
root="$PWD"

# Use the repo-local cargo home (and offline mode) only when it has been
# populated; a fresh clone falls back to normal cargo with network access.
offline=""
if [ -d "$root/.cargo-home/registry" ]; then
    export CARGO_HOME="$root/.cargo-home"
    offline="--offline"
fi

version=$(cargo metadata --no-deps --format-version 1 --locked $offline |
    python3 -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "mathstral":
        print(p["version"])
        break
else:
    sys.exit("error: mathstral package not found")
')

cargo build $offline --release --locked -p mathstral

bundle="$root/target/Mathstral.app"
contents="$bundle/Contents"
mkdir -p "$contents/MacOS"

# Replace the binary atomically so a running copy is never left half-written.
cp target/release/mathstral "$contents/MacOS/Mathstral.next"
mv -f "$contents/MacOS/Mathstral.next" "$contents/MacOS/Mathstral"

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Mathstral</string>
<key>CFBundleIdentifier</key><string>local.mathstral.editor</string>
<key>CFBundleName</key><string>Mathstral</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHumanReadableCopyright</key><string>Mathstral</string>
</dict></plist>
PLIST
plutil -lint "$contents/Info.plist" >/dev/null

# Legal notices and font attribution, rebuilt from scratch every time.
rm -rf "$contents/Resources"
mkdir -p "$contents/Resources"
cp LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.md "$contents/Resources/"
cp assets/fonts/README.md "$contents/Resources/FONTS.md"
cp assets/fonts/UPSTREAM-NOTICE "$contents/Resources/FONTS-UPSTREAM-NOTICE"

codesign --force --deep --sign - "$bundle"

printf '%s\n' "$bundle"
