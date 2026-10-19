#!/usr/bin/env bash
# Susun MiniLyrics.app dari binary hasil build, lalu codesign ad-hoc.
#
# Pakai:
#   scripts/bundle-macos.sh [binary] [folder-output]
#   scripts/bundle-macos.sh                       # target/release/mini-lyrics → dist/
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${1:-$ROOT/target/release/mini-lyrics}"
OUT="${2:-$ROOT/dist}"
APP="$OUT/MiniLyrics.app"

if [[ ! -f "$BIN" ]]; then
  echo "binary tidak ada: $BIN (jalankan dulu: cargo build --release -p mini-lyrics)" >&2
  exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)"
BUILD="${MINI_LYRICS_COMMIT_SHA:-$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo dev)}"
BUILD="${BUILD:0:7}"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/mini-lyrics"
chmod +x "$APP/Contents/MacOS/mini-lyrics"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>MiniLyrics</string>
  <key>CFBundleDisplayName</key>
  <string>MiniLyrics</string>
  <key>CFBundleIdentifier</key>
  <string>dev.minilyrics.app</string>
  <key>CFBundleExecutable</key>
  <string>mini-lyrics</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>MiniLyricsCommit</key>
  <string>${BUILD}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <!-- App aksesori: tanpa ikon Dock. -->
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSAppleEventsUsageDescription</key>
  <string>MiniLyrics membaca lagu yang sedang diputar di Spotify untuk menampilkan lirik tersinkron.</string>
</dict>
</plist>
PLIST

if command -v plutil >/dev/null 2>&1; then
  plutil -lint "$APP/Contents/Info.plist"
fi

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP"
  codesign --verify --verbose=2 "$APP"
else
  echo "codesign tidak ada (bukan macOS?), bundle tidak ditandatangani" >&2
fi

echo "OK: $APP"
