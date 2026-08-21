#!/bin/bash
# DownPilot native messaging host installer
# Installs the NMH manifest for Chrome, Chromium, Vivaldi, Edge, Brave.
# Usage: ./install.sh [extension_id]
#        Defaults to the pinned ID from the extension's manifest.json key.
#        (The pinned key makes the ID stable, so the default is correct.)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MANIFEST_SRC="$SCRIPT_DIR/com.downpilot.nmh.json"
# Stable ID derived from the pinned key in extension/manifest.json
EXTENSION_ID="${1:-onmpegdpcecafeebchmmmohbdbjfcepb}"

# Platforms (Chrome-based browsers on Linux)
BROWSER_DIRS=(
  "/home/amir/.config/google-chrome/NativeMessagingHosts"
  "/home/amir/.config/chromium/NativeMessagingHosts"
  "/home/amir/.config/vivaldi/NativeMessagingHosts"
  "/home/amir/.config/msedge/NativeMessagingHosts"
  "/home/amir/.config/brave/NativeMessagingHosts"
)

# Build the relay binary (needs Rust toolchain)
echo "Building downpilot-nmh relay binary..."
cd "$SCRIPT_DIR/../src-tauri"
cargo build -p downpilot-nmh --release 2>&1

# Copy binary to ~/.local/bin/
mkdir -p "/home/amir/.local/bin"
cp "$SCRIPT_DIR/../src-tauri/target/release/downpilot-nmh" "/home/amir/.local/bin/downpilot-nmh"
echo "  -> /home/amir/.local/bin/downpilot-nmh"

# Install the manifest for each browser
for dir in "${BROWSER_DIRS[@]}"; do
  mkdir -p "$dir"
  # Patch the extension ID into the manifest
  sed "s/YOUR_EXTENSION_ID_HERE/$EXTENSION_ID/g" "$MANIFEST_SRC" > "$dir/com.downpilot.nmh.json"
  # Update the binary path to the absolute path
  sed -i "s|\"path\":.*|\"path\": \"/home/amir/.local/bin/downpilot-nmh\",|" "$dir/com.downpilot.nmh.json"
  echo "  -> $dir/com.downpilot.nmh.json"
done

# Also write a copy to the project native-host dir with the real ID for future use
sed "s/YOUR_EXTENSION_ID_HERE/$EXTENSION_ID/g" "$MANIFEST_SRC" > "$SCRIPT_DIR/com.downpilot.nmh.final.json"
sed -i "s|\"path\":.*|\"path\": \"/home/amir/.local/bin/downpilot-nmh\",|" "$SCRIPT_DIR/com.downpilot.nmh.final.json"
echo "-> $SCRIPT_DIR/com.downpilot.nmh.final.json (with resolved ID)"

echo ""
echo "Done! Extension ID used: $EXTENSION_ID"
echo ""
echo "To find your extension ID:"
echo "  1. Go to chrome://extensions"
echo "  2. Enable Developer mode (top-right toggle)"
echo "  3. Load unpacked extension from the 'extension/' directory"
echo "  4. Copy the ID shown in the card"
echo "  5. Re-run: ./install.sh <that_id>"
echo ""
echo "If you already have the ID, re-run: ./install.sh <id>"