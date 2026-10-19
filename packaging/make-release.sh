#!/usr/bin/env bash
# DownPilot v1.0 — Linux packaging script
#
# Builds a self-contained release archive:
#   DownPilot-1.0.0-linux-x86_64.tar.gz
#
# The archive contains everything needed to install WITHOUT the dev repo:
#   downpilot            main binary (frontend embedded)
#   downpilot-nmh        native-messaging relay binary
#   downpilot.desktop    visible desktop launcher entry
#   downpilot-handler.desktop  deep-link (downpilot://) handler entry
#   icons/               32x32, 128x128, 256x256 application icons
#   native-host/com.downpilot.nmh.json        NMH manifest template (Chromium)
#   native-host/firefox/…                     NMH manifest (Firefox)
#   install.sh           installer (binary + launcher + desktop + icons + NMH)
#   uninstall.sh         uninstaller (app files only; NEVER touches downloads)
#   UNINSTALL.md         what gets removed vs what is kept
#   DOWNPILOT_V1_RELEASE_NOTES.md
#   DOWNPILOT_V1_RELEASE_MANIFEST.md
#
# Prereqs on the BUILD machine only: cargo + node (not needed on target).

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
VER="1.0.0"
STAGE="$REPO/release/DownPilot-${VER}-linux-x86_64"
NAME="DownPilot-${VER}-linux-x86_64.tar.gz"

echo "==> Staging $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE/native-host/firefox" "$STAGE/icons" "$STAGE/share"

# 1. Binaries (must already be built: cargo build --release, tauri build)
cp "$REPO/src-tauri/target/release/downpilot"     "$STAGE/downpilot"
cp "$REPO/src-tauri/target/release/downpilot-nmh" "$STAGE/downpilot-nmh"
chmod +x "$STAGE/downpilot" "$STAGE/downpilot-nmh"

# 2. Icons
cp "$REPO/src-tauri/icons/32x32.png"   "$STAGE/icons/downpilot-32.png"
cp "$REPO/src-tauri/icons/128x128.png" "$STAGE/icons/downpilot-128.png"
cp "$REPO/src-tauri/icons/icon.png"    "$STAGE/icons/downpilot-256.png"

# 3. NMH manifests (templates with $HOME resolved at install time)
sed "s|/home/amir|\$HOME|g" "$REPO/native-host/com.downpilot.nmh.json" \
  > "$STAGE/native-host/com.downpilot.nmh.json"
sed "s|/home/amir|\$HOME|g" "$REPO/native-host/firefox/downpilot-firefox-nmh.json" \
  > "$STAGE/native-host/firefox/downpilot-firefox-nmh.json"

# 4. Docs
cp "$REPO/DOWNPILOT_V1_RELEASE_NOTES.md"   "$STAGE/"
cp "$REPO/DOWNPILOT_V1_RELEASE_MANIFEST.md" "$STAGE/"

# 5. Desktop entries (installed to ~/.local/share/applications)
cat > "$STAGE/share/downpilot.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=DownPilot
GenericName=Download Manager
Comment=Fast, segmented download manager with queues, scheduling and browser capture
Exec=\$HOME/.local/bin/downpilot %u
Icon=downpilot
Terminal=false
Categories=Network;FileTransfer;
Keywords=download;downloader;transfer;
StartupWMClass=DownPilot
MimeType=x-scheme-handler/downpilot;
EOF

# The hidden deep-link handler (no menu entry; only the protocol)
cat > "$STAGE/share/downpilot-handler.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=DownPilot URL Handler
NoDisplay=true
Exec=\$HOME/.local/bin/downpilot %u
MimeType=x-scheme-handler/downpilot;
Terminal=false
EOF

# 6. Launcher wrapper (mirrors the dev machine's proven wrapper)
cat > "$STAGE/downpilot-launcher" <<'EOF'
#!/bin/bash
# DownPilot launcher — forces X11 backend for WebKitGTK under Wayland.
export GDK_BACKEND=x11
export WEBKIT_DISABLE_DMABUF_RENDERER=1
exec "$HOME/.local/libexec/downpilot-real" "$@"
EOF
chmod +x "$STAGE/downpilot-launcher"

# 7. Installer
cat > "$STAGE/install.sh" <<'INST'
#!/usr/bin/env bash
# DownPilot installer — user-local install, no root required.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$HOME/.local/libexec" "$HOME/.local/bin" \
         "$HOME/.local/share/applications" \
         "$HOME/.local/share/icons/hicolor/32x32/apps" \
         "$HOME/.local/share/icons/hicolor/128x128/apps" \
         "$HOME/.local/share/icons/hicolor/256x256/apps"

echo "==> Installing binaries"
install -m755 "$HERE/downpilot"     "$HOME/.local/libexec/downpilot-real"
install -m755 "$HERE/downpilot-nmh" "$HOME/.local/bin/downpilot-nmh"

echo "==> Installing launcher"
install -m755 "$HERE/downpilot-launcher" "$HOME/.local/bin/downpilot"

echo "==> Installing desktop entries"
sed "s|\$HOME|$HOME|g" "$HERE/share/downpilot.desktop" \
  > "$HOME/.local/share/applications/downpilot.desktop"
sed "s|\$HOME|$HOME|g" "$HERE/share/downpilot-handler.desktop" \
  > "$HOME/.local/share/applications/downpilot-handler.desktop"

echo "==> Installing icons"
install -m644 "$HERE/icons/downpilot-32.png"  "$HOME/.local/share/icons/hicolor/32x32/apps/downpilot.png"
install -m644 "$HERE/icons/downpilot-128.png" "$HOME/.local/share/icons/hicolor/128x128/apps/downpilot.png"
install -m644 "$HERE/icons/downpilot-256.png" "$HOME/.local/share/icons/hicolor/256x256/apps/downpilot.png"

echo "==> Registering downpilot:// handler"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "==> Installing native-messaging host manifests (Chromium + Firefox)"
for d in "$HOME/.config/google-chrome/NativeMessagingHosts" \
         "$HOME/.config/chromium/NativeMessagingHosts" \
         "$HOME/.config/vivaldi/NativeMessagingHosts" \
         "$HOME/.config/brave/NativeMessagingHosts" \
         "$HOME/.config/msedge/NativeMessagingHosts"; do
  mkdir -p "$d"
  sed "s|\$HOME|$HOME|g" "$HERE/native-host/com.downpilot.nmh.json" > "$d/com.downpilot.nmh.json"
done
mkdir -p "$HOME/.mozilla/native-messaging-hosts"
sed "s|\$HOME|$HOME|g" "$HERE/native-host/firefox/downpilot-firefox-nmh.json" \
  > "$HOME/.mozilla/native-messaging-hosts/com.downpilot.nmh.json"

cat <<'TIP'

Install complete.
  Launch:    downpilot   (or from your desktop menu)
  Browser:   load the DownPilot extension (see RELEASE notes §Browser Integration),
             then confirm the NMH manifest's allowed extension ID matches yours:
             ~/.config/google-chrome/NativeMessagingHosts/com.downpilot.nmh.json
TIP
INST
chmod +x "$STAGE/install.sh"

# 8. Uninstaller — removes app files ONLY; never touches user data
cat > "$STAGE/uninstall.sh" <<'UNIN'
#!/usr/bin/env bash
# DownPilot uninstaller.
# Removes: binaries, launcher, desktop entries, icons, NMH manifests.
# NEVER removes: your downloaded files, the DownPilot database, or settings.
set -euo pipefail

echo "==> Removing application files"
rm -f "$HOME/.local/libexec/downpilot-real" \
      "$HOME/.local/bin/downpilot" \
      "$HOME/.local/bin/downpilot-nmh" \
      "$HOME/.local/share/applications/downpilot.desktop" \
      "$HOME/.local/share/applications/downpilot-handler.desktop" \
      "$HOME/.local/share/icons/hicolor/32x32/apps/downpilot.png" \
      "$HOME/.local/share/icons/hicolor/128x128/apps/downpilot.png" \
      "$HOME/.local/share/icons/hicolor/256x256/apps/downpilot.png"

for d in "$HOME/.config/google-chrome/NativeMessagingHosts" \
         "$HOME/.config/chromium/NativeMessagingHosts" \
         "$HOME/.config/vivaldi/NativeMessagingHosts" \
         "$HOME/.config/brave/NativeMessagingHosts" \
         "$HOME/.config/msedge/NativeMessagingHosts" \
         "$HOME/.mozilla/native-messaging-hosts"; do
  rm -f "$d/com.downpilot.nmh.json"
done

update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true

cat <<'MSG'

Application removed. Your data was NOT touched:
  downloads  : wherever you saved them (default ~/Downloads and your categories)
  database   : ~/.local/share/com.downpilot.app/downpilot.db
  settings   : ~/.local/share/com.downpilot.app/ (settings table)
  credentials: system keyring/credential store entries

To erase ALL DownPilot data as well, run additionally:
  rm -rf ~/.local/share/com.downpilot.app
MSG
UNIN
chmod +x "$STAGE/uninstall.sh"

# 9. UNINSTALL.md
cat > "$STAGE/UNINSTALL.md" <<'DOC'
# Uninstalling DownPilot

Run: `./uninstall.sh`

| What | Removed? | Where |
|---|---|---|
| Application binary | Yes | `~/.local/libexec/downpilot-real` |
| Launcher + NMH relay | Yes | `~/.local/bin/downpilot{,-nmh}` |
| Desktop entries + icons | Yes | `~/.local/share/applications`, `~/.local/share/icons` |
| NMH manifests | Yes | browser `NativeMessagingHosts` dirs |
| **Downloaded files** | **NEVER** | your save dirs (default `~/Downloads`) |
| Database (history/settings) | Kept | `~/.local/share/com.downpilot.app/downpilot.db` |
| Credentials | Kept | system credential store |

To also erase data: `rm -rf ~/.local/share/com.downpilot.app` (see uninstall.sh output).
DOC

# 10. Tarball
echo "==> Packing $NAME"
tar -C "$(dirname "$STAGE")" -czf "$REPO/release/$NAME" "$(basename "$STAGE")"
sha256sum "$REPO/release/$NAME" | tee "$REPO/release/$NAME.sha256"
echo "==> Done: $REPO/release/$NAME"
