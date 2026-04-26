#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="HQ"
BUNDLE_ID="net.yulab.project-hq"
MIN_SYSTEM_VERSION="14.0"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"
APP_BINARY="$APP_MACOS/$APP_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
ICON_FILE="$ROOT_DIR/macos/Assets/AppIcon.icns"
HQ_DATA_DIR="${HQ_DIR:-$HOME/git/hq}"
HQ_PORT="${HQ_DESKTOP_PORT:-3001}"

pkill -x "$APP_NAME" >/dev/null 2>&1 || true
pkill -f "$APP_BUNDLE/Contents/Resources/hq --dir $HQ_DATA_DIR serve --port" >/dev/null 2>&1 || true

cd "$ROOT_DIR"
cargo build --release --bin hq
swift build -c release
./script/make_icon.swift

BUILD_BINARY="$(swift build -c release --show-bin-path)/$APP_NAME"
HQ_BINARY="$ROOT_DIR/target/release/hq"

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS" "$APP_RESOURCES"
cp "$BUILD_BINARY" "$APP_BINARY"
cp "$HQ_BINARY" "$APP_RESOURCES/hq"
cp "$ICON_FILE" "$APP_RESOURCES/AppIcon.icns"
chmod +x "$APP_BINARY" "$APP_RESOURCES/hq"

cat >"$INFO_PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$APP_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundleDisplayName</key>
  <string>HQ</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
  <key>HQDataDir</key>
  <string>$HQ_DATA_DIR</string>
  <key>HQPort</key>
  <string>$HQ_PORT</string>
</dict>
</plist>
PLIST

open_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

case "$MODE" in
  run)
    open_app
    ;;
  --debug|debug)
    HQ_DIR="$HQ_DATA_DIR" HQ_DESKTOP_PORT="$HQ_PORT" lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    open_app
    /usr/bin/log stream --info --style compact --predicate "subsystem == \"$BUNDLE_ID\""
    ;;
  --verify|verify)
    open_app
    pgrep -x "$APP_NAME" >/dev/null
    for _ in {1..40}; do
      if /usr/bin/curl --silent --fail "http://127.0.0.1:$HQ_PORT/" >/dev/null; then
        exit 0
      fi
      sleep 0.25
    done
    echo "HQ app launched, but http://127.0.0.1:$HQ_PORT/ did not become ready" >&2
    exit 1
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
