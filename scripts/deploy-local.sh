#!/usr/bin/env bash
# Build release .app and install into ~/Applications for local testing.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="FanFiction Reader"
DEST="$HOME/Applications"
BUNDLE="src-tauri/target/release/bundle/macos/$APP_NAME.app"

bun run tauri build --bundles app

# Quit running copy so the replace takes effect.
osascript -e "tell application \"$APP_NAME\" to quit" 2>/dev/null || true
sleep 1

mkdir -p "$DEST"
/bin/rm -rf "$DEST/$APP_NAME.app"
/bin/cp -R "$BUNDLE" "$DEST/"
echo "Installed: $DEST/$APP_NAME.app"
open "$DEST/$APP_NAME.app"
