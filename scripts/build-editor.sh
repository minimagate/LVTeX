#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo-home"
cargo build --offline --release --locked -p mathstral
bundle="$PWD/target/Mathstral.app"
mkdir -p "$bundle/Contents/MacOS"
cp target/release/mathstral "$bundle/Contents/MacOS/Mathstral.next"
mv -f "$bundle/Contents/MacOS/Mathstral.next" "$bundle/Contents/MacOS/Mathstral"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Mathstral</string>
<key>CFBundleIdentifier</key><string>local.mathstral.editor</string>
<key>CFBundleName</key><string>Mathstral</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHumanReadableCopyright</key><string>Mathstral</string>
</dict></plist>
PLIST
printf '%s\n' "$bundle"
