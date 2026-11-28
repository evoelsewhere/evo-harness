#!/usr/bin/env bash
# The "Evo Mates" entry in Applications on macOS (docs/features/evo-mates,
# FR-1, NFR-4; ADR 0028). macOS shows one Dock icon per process, so Evo Mates
# is a tiny launcher bundle: it starts the Evo app with `--app mates`, and
# the running copy (single-instance) opens the Mates window.
#
#   make-macos-launcher.sh <out dir> <app bundle id> <icns file> [version]
#
# Signs with $SIGN_IDENTITY when set (hardened runtime, timestamp), else
# ad hoc.
set -euo pipefail

out_dir="$1"
bundle_id="$2"
icns="$3"
version="${4:-1.0.0}"

app="$out_dir/Evo Mates.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$icns" "$app/Contents/Resources/mates.icns"

cat > "$app/Contents/MacOS/evo-mates" <<SH
#!/bin/sh
# -n: a new process, which hands --app mates to the running Evo and exits.
exec /usr/bin/open -n -b "$bundle_id" --args --app mates
SH
chmod 755 "$app/Contents/MacOS/evo-mates"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key><string>Evo Mates</string>
  <key>CFBundleName</key><string>Evo Mates</string>
  <key>CFBundleIdentifier</key><string>${bundle_id}.mates-launcher</string>
  <key>CFBundleExecutable</key><string>evo-mates</string>
  <key>CFBundleIconFile</key><string>mates</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$app"
else
  codesign --force --sign - "$app"
fi
codesign --verify --verbose=2 "$app"
echo "wrote $app"
