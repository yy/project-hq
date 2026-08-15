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
INSTALLED_APP="/Applications/$APP_NAME.app"
HQ_DATA_DIR="${HQ_DIR:-$HOME/git/hq}"

# Distributable builds must not bake in this machine's data dir; the app's
# first-run welcome screen offers to create ~/Documents/HQ instead.
if [[ "$MODE" == "--dist" || "$MODE" == "dist" ]]; then
  HQ_DATA_DIR="~/Documents/HQ"
fi

cd "$ROOT_DIR"
cargo build --release --bin hq
swift build -c release

BUILD_BINARY="$(swift build -c release --show-bin-path)/$APP_NAME"
HQ_BINARY="$ROOT_DIR/target/release/hq"

if [[ -e "$APP_BUNDLE" ]]; then
  trash "$APP_BUNDLE"
fi
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
</dict>
</plist>
PLIST

stop_app() {
  pkill -x "$APP_NAME" >/dev/null 2>&1 || true
  pkill -f "$APP_BUNDLE/Contents/Resources/hq" >/dev/null 2>&1 || true
  pkill -f "$INSTALLED_APP/Contents/Resources/hq" >/dev/null 2>&1 || true

  for _ in {1..40}; do
    if ! pgrep -x "$APP_NAME" >/dev/null; then
      return
    fi
    sleep 0.1
  done
  echo "Could not stop the existing $APP_NAME app" >&2
  exit 1
}

install_app() {
  stop_app
  # /Applications carries the sunlnk flag, so unlinking the installed bundle
  # needs root. Copy over it in place when the removal is refused.
  if [[ -e "$INSTALLED_APP" ]]; then
    trash "$INSTALLED_APP" 2>/dev/null || true
  fi
  /usr/bin/ditto "$APP_BUNDLE" "$INSTALLED_APP"
}

open_app() {
  install_app
  /usr/bin/open "$INSTALLED_APP"
}

case "$MODE" in
  run)
    open_app
    ;;
  --build|build)
    echo "Built $APP_BUNDLE"
    ;;
  --debug|debug)
    stop_app
    HQ_DIR="$HQ_DATA_DIR" lldb -- "$APP_BINARY"
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
    APP_PID="$(pgrep -x "$APP_NAME" | head -n 1 || true)"
    if [[ -z "$APP_PID" ]] || [[ "$(ps -p "$APP_PID" -o command=)" != "$INSTALLED_APP/Contents/MacOS/$APP_NAME" ]]; then
      echo "$APP_NAME did not launch from $INSTALLED_APP" >&2
      exit 1
    fi
    for _ in {1..40}; do
      SERVER_PID="$(pgrep -f "$INSTALLED_APP/Contents/Resources/hq --dir $HQ_DATA_DIR serve --port 0 --auth-token" | head -n 1 || true)"
      if [[ -n "$SERVER_PID" ]]; then
        if /usr/sbin/lsof -nP -a -p "$SERVER_PID" -iTCP -sTCP:LISTEN | grep -q LISTEN; then
          exit 0
        fi
      fi
      sleep 0.25
    done
    echo "HQ app launched, but its private server did not become ready" >&2
    exit 1
    ;;
  --dist|dist)
    ZIP_PATH="$DIST_DIR/HQ.zip"
    if [[ -e "$ZIP_PATH" ]]; then
      trash "$ZIP_PATH"
    fi
    ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"
    echo "Built $ZIP_PATH (data dir defaults to ~/Documents/HQ; first run shows the welcome screen)"
    ;;
  *)
    echo "usage: $0 [run|--build|--debug|--logs|--telemetry|--verify|--dist]" >&2
    exit 2
    ;;
esac
