# DownPilot

A high-performance, open-source Internet Download Manager for Linux — built
with **Tauri v2 (Rust + React + Tailwind)** and designed for **Bazzite**
(immutable Fedora, Wayland, Flatpak/AppImage-friendly).

```
┌─ DownPilot ──────────────────────────────────────────────┐
│  Sidebar   │  Toolbar (New, Search, Speed, Theme)        │
│  ────────  │  ────────────────────────────────────────   │
│  All       │  Name          Status   Progress   Speed    │
│  Downloading│  file.iso      ▓▓▓▓░ 62%  4.2 MB/s  ...   │
│  Paused    │  video.mp4     ▓▓▓░  38%  1.1 MB/s  ...   │
│  Completed │  archive.zip   Done   100%         ✓       │
│  Compressed│                                             │
│  Documents │  (IDM-style dense table, inline progress)   │
│  Music     │                                             │
│  Video     │                                             │
│  Programs  │  Status bar: 2 active · 34 MB/s total       │
└──────────────────────────────────────────────────────────┘
```

## Architecture

```
Chrome extension (MV3)
  ├─ content.js   — floating overlay button on video players,
  │                quality dropdown (Best / 1080p / 720p / Audio)
  ├─ background.js— context menus, download interception
  └─ native messaging host (downpilot-nmh binary)
        │  stdin/stdout JSON frames
        ▼
src-tauri (Rust)
  ├─ lib.rs            — Tauri commands + NMH TCP server (127.0.0.1:19851)
  └─ crates/
     ├─ downpilot-core — the engine: chunked HTTP Range downloads,
     │                   pause/resume via .part files, SQLite persistence,
     │                   yt-dlp/ffmpeg media probing + downloads,
     │                   global speed limiting, SHA-256 verification
     └─ downpilot-nmh  — relay binary installed for Chrome
        ▲
src/ (React + Tailwind)
  └─ theme engine — CSS variables, 4 palettes (Deep Midnight/OLED,
                    Modern Dark, Clean Light, Cyberpunk), accent picker
```

- **Chunked downloads**: file split into N ranges (4–16 threads, configurable),
  each fetched with `Range: bytes=…`, written to `partN` files, merged on
  completion. Resume continues from part file sizes.
- **Persistence**: SQLite (WAL) in
  `~/.local/share/com.downpilot.app/downpilot.db`; every state change and
  throttled progress write survives app restarts. Tasks stuck in
  `downloading` after a crash are reset to `paused` on startup.
- **Media grabbing**: `yt-dlp -J` probe → curated quality list → download
  via yt-dlp (HLS/DASH muxing with ffmpeg).
- **Browser bridge**: Chrome native messaging → `downpilot-nmh` relay →
  loopback TCP `127.0.0.1:19851` → engine. No ports exposed beyond localhost,
  no daemon needed — the NMH relay auto-launches the app if it's closed.

## Prerequisites (Bazzite)

```bash
# Node + npm (usually preinstalled); yt-dlp + ffmpeg:
sudo rpm-ostree install ffmpeg   # if not present
pipx install yt-dlp              # or: flatpak install flathub io.github.yt_dlp.yt-dlp

# Rust toolchain (user-space, no reboot needed):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Tauri v2 system deps (webkit2gtk etc.) — REQUIRES LAYERING + REBOOT:
sudo rpm-ostree install webkit2gtk4.1-devel openssl-devel \
  libappindicator-gtk3-devel librsvg2-devel \
  glib2-devel gtk3-devel gdk-pixbuf2-devel \
  patchelf librsvg2 libappindicator-gtk3
sudo reboot
```

## Build & run

```bash
cd ~/projects/downpilot

# 1. Frontend
cd src && npm install && npm run build && cd ..

# 2. Rust engine + NMH relay (no system deps needed)
cd src-tauri && cargo build --release -p downpilot-nmh
cp target/release/downpilot-nmh ~/.local/bin/

# 3. Desktop app (needs the layered webkit2gtk from above)
cargo tauri build            # full bundles (deb/rpm/AppImage)
# or dev mode:
npm run dev --prefix ../src & cargo tauri dev

# 4. Engine tests (no GUI needed — runs against a local test HTTP server)
cd crates/downpilot-core && cargo test
```

## Chrome extension setup

The extension is pre-wired with a pinned key so its ID is stable:
**`onmpegdpcecafeebchmmmohbdbjfcepb`**. The native messaging host is already
registered for both Chrome (flatpak) and Chromium.

### Load the extension

1. Open `chrome://extensions`, enable **Developer mode**, click **Load unpacked**,
   select the `extension/` directory.
2. The ID will match automatically — no extra registration needed.
3. Start DownPilot, then click the extension icon. The popup shows a green
   "Connected" dot when the native messaging relay is active.

### How it's wired

```
Chrome extension (ID: onmpegdpcecafeebchmmmohbdbjfcepb)
        ↓ chrome.runtime.connectNative("com.downpilot.nmh")
Native host manifest → /var/home/.../NativeMessagingHosts/com.downpilot.nmh.json
        ↓ stdio
~/.var/app/com.google.Chrome/.../com.downpilot.nmh/downpilot-nmh  (relay)
        ↓ TCP 127.0.0.1:19851
DownPilot desktop app
```

- **Chrome (flatpak)**: host manifest at
  `~/.var/app/com.google.Chrome/config/google-chrome/NativeMessagingHosts/`
  Points to a copy of the relay inside the same flatpak data dir.
- **Chromium (native)**: host manifest at
  `~/.config/chromium/NativeMessagingHosts/` — points to `~/.local/bin/downpilot-nmh`.

### What the extension does

- **Overlay button**: hovering any `<video>` shows a discreet "DownPilot"
  pill in the player corner. Click → quality dropdown (Best, 1080p, 720p,
  Audio only) → task is sent straight to the app.
- **Context menu**: right-click any link → "Download with DownPilot";
  right-click a page with a video → "Grab media from this page".
- **Interception** (optional, toggle in popup): when on, Chrome's own download
  is cancelled and the URL is handed to DownPilot's multi-threaded engine
  instead. If DownPilot isn't running, the download falls back to Chrome.

## Firefox support

Firefox MV3 (115+) supports native messaging the same way Chrome does,
so the **same** `downpilot-nmh` relay binary in the Tauri app handles
both browsers — the protocol is identical (Chrome native messaging
frames on stdio, length-prefixed JSON, terminated by the same
newline-delimited frame format the Rust side reads in
`run_nmh_server`/`read_frame`). No Tauri source change is required.

There are three things to install:

1. **The Firefox extension** — load `extension/firefox/manifest.json`
   as a temporary add-on:
   - Open `about:debugging#/runtime/this-firefox`
   - Click **Load Temporary Add-on…**
   - Select `extension/firefox/manifest.json`
   The temporary install is removed when Firefox restarts; re-load
   from the same dialog to keep it.
2. **The Firefox native messaging host manifest** — install the file
   at `native-host/firefox/downpilot-firefox-nmh.json` to
   `~/.mozilla/native-messaging-hosts/` (create the directory if it
   doesn't exist). The manifest's `path` field points to the same
   `~/.local/bin/downpilot-nmh` binary the Chrome manifest uses; no
   separate copy of the relay is needed.
3. **The Tauri app** — unchanged. Start it once so the relay can
   connect to `127.0.0.1:19851`.

**Important:** the `browser_specific_settings.gecko.id` in
`extension/firefox/manifest.json` (`downpilot@local`) MUST equal the
`allowed_extensions` entry in the host manifest
(`downpilot-firefox-nmh.json`). Firefox silently refuses to connect
to the native host if these don't match — there is no error in the
UI, the extension just shows a permanent "not connected" state. If
you change the id in one place, change it in both.

The Firefox manifest uses `browser_action` (not `action`), a
`background.scripts` array (not `service_worker`), and a
`browser_specific_settings.gecko` block (not a `key`). All other
fields are kept identical to the Chrome manifest, and `background.js`
is copied verbatim because Firefox aliases `chrome.*` to `browser.*`
under the gecko shim. See `extension/firefox/README.md` for the
full Chrome → Firefox diff.

## Themes

Settings → Themes. Four presets + accent color picker (custom hex or the
10 built-in accents). Themes are pure CSS custom properties, so switching is
instant and every widget follows automatically:

| Preset            | Palette                                |
|-------------------|----------------------------------------|
| Deep Midnight     | OLED black `#000`, subtle surfaces     |
| Modern Dark       | Graphite blue-grey                     |
| Clean Light       | White / light grey                     |
| Cyberpunk         | Near-black purple + neon accent        |

Accents: Bazzite Purple (default), Cyan, Green, Orange, Pink, Red, Blue,
Yellow, Teal, Violet.

## Project layout

```
src/                  React + Tailwind UI (theming, table, dialogs, settings)
src-tauri/            Rust app shell (commands, NMH TCP server, window)
src-tauri/crates/downpilot-core/   engine (downloads, db, yt-dlp, models)
src-tauri/crates/downpilot-nmh/    Chrome native messaging relay
extension/            Chrome MV3 extension (overlay, context menu, popup)
native-host/          NMH manifest + installer
scripts/gen_icons.py  icon generator (pure stdlib, no deps)
```

## Status

- [x] Multi-threaded chunked downloads (4–16 threads, HTTP Range)
- [x] Pause / resume (verified: part-file offsets survive restart)
- [x] SQLite persistence (WAL, crash-safe)
- [x] SHA-256 integrity verification
- [x] Global speed limiting
- [x] yt-dlp probe + media download (HLS/DASH via ffmpeg)
- [x] Native messaging host (Chrome ↔ engine bridge)
- [x] Video overlay button + quality dropdown (extension)
- [x] Context menu + download interception (extension)
- [x] Multi-theme engine with accent picker
- [x] IDM-style dense UI (categories sidebar, table, status bar)
- [ ] `cargo tauri build` bundles (needs webkit2gtk layer — see above)
- [ ] Auto-update / flatpak packaging polish
```
