#!/usr/bin/env bash
# Build the release .app and install it into ~/Applications.
# Identical in every Tauri app; the app name comes from src-tauri/tauri.conf.json.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="$(node -p "require('./src-tauri/tauri.conf.json').productName")"
DEST="$HOME/Applications"
BUNDLE="src-tauri/target/release/bundle/macos/$APP_NAME.app"

bun run tauri:build --bundles app

# Quit the running copy so the replace takes effect.
osascript -e "tell application \"$APP_NAME\" to quit" 2>/dev/null || true
sleep 1

mkdir -p "$DEST"
/bin/rm -rf "$DEST/$APP_NAME.app"
/bin/cp -R "$BUNDLE" "$DEST/"
echo "Installed: $DEST/$APP_NAME.app"

# A same-named copy in /Applications is what Spotlight and the Dock may launch instead, and
# it stays at whatever version it was last built. Warn only: it may be one you meant to keep.
if [ -d "/Applications/$APP_NAME.app" ]; then
  echo "WARNING: a second copy exists at /Applications/$APP_NAME.app and is not updated by deploy."
  echo "         Remove it: /bin/rm -rf \"/Applications/$APP_NAME.app\""
fi
open "$DEST/$APP_NAME.app"
