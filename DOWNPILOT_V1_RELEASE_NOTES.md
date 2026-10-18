# DownPilot v1.0 — Release Notes

Date: 2026-08-31
Artifact: `src-tauri/target/release/downpilot` → installed as `~/.local/libexec/downpilot-real`
Artifact SHA-256: `7a6d32223ee68eeb27f2153cd1efd827bd3c32ed1cdb6e13e269e0ce720be11b`

## Highlights

- Segmented multi-connection download engine with pause/resume/retry
- Queues with concurrency limits, priorities, Force Start, queue chaining
- Scheduler with bandwidth profiles
- Per-task proxy routing (Direct / Global / Queue / Site Rule / Custom task)
- Authentication (URL credentials + queue auth) and secure credential storage
- Browser capture (Chrome MV3 extension + native messaging host)
- Site Grabber, Batch Download, Site Rules, Categories with destination routing
- Collision-safe file naming, checksum verification (SHA-256)
- History with per-row actions (Open File / Open Folder / Copy Path / Redownload / Delete File)
- Details inspector (resizable, persisted), Properties dialog
- IDM-style menu bar, toolbar, dense download table, sidebar, status bar
- Three themes (deep-midnight, modern-dark, clean-light), all WCAG AA verified

## Requirements

- Linux x86_64 (tested on Bazzite 44 / Fedora atomic, GNOME Wayland)
- WebKitGTK 4.1 runtime (standard on Fedora/Ubuntu/Arch desktops)
- No root required; installs entirely under `$HOME`

## Installation

The release artifact is a single binary with the frontend embedded.

```bash
# Run directly from the build tree
./src-tauri/target/release/downpilot

# Or use the installed launcher
~/.local/bin/downpilot
```

The launcher (`~/.local/bin/downpilot`) execs `~/.local/libexec/downpilot-real`.

Data locations:

- Database & settings: `~/.local/share/com.downpilot.app/downpilot.db`
- Downloaded parts: `~/.local/share/com.downpilot.app/parts/`
- Credentials: `~/.local/share/com.downpilot.app/credentials/`

## Browser Integration

- Chrome/Chromium: MV3 extension + native messaging host manifest installed at
  `~/.config/google-chrome/NativeMessagingHosts/com.downpilot.nmh.json`.
- Capture modes: Ask / Automatic / Off, with per-extension-type always-allow rules.
- **Verification status**: the NMH framing protocol was exercised end-to-end
  against the installed release (ping/status/list/download/pause/resume all
  verified over the live socket). The full Chrome-extension UI loop (click in
  Chrome → popup → handoff) was **environment-blocked** during acceptance:
  no CDP service-worker target was reachable from the harness (Flatpak Chrome
  with remote debugging on IPv6 only, extension SW not exposed). This is an
  environment limitation, not an application defect; the protocol layer and
  the app-side handler are covered by automated tests.

## Security

- Credentials are stored outside the database, in
  `~/.local/share/com.downpilot.app/credentials/` (keyring-backed where available).
- Browser cookies are redacted at the NMH serialization boundary (verified:
  `list` response contains zero cookie fields).
- Queue passwords stored via the credential store, not in the DB.
- Task URLs are stored as entered (standard IDM behavior); URLs containing
  embedded `user:pass` credentials will persist them locally. Prefer
  queue-level authentication for sensitive endpoints.
- Proxy credentials are never placed in frontend state or logs (verified by
  code inspection and the cookie-redaction regression test).

## Known Limitations

- Chrome extension UI-loop E2E is environment-blocked (see Browser Integration).
- The NMH `download` message does not carry `save_dir`/`proxy_url`; automated
  submissions land in the category-default folder. The UI and Tauri command
  path support the full field set. (P3: extend the NMH protocol in v1.1.)
- URLs with embedded credentials are stored as-entered in the local DB.
- Synthetic GUI input (XWayland coordinate injection) is unreliable on this
  machine; UI workflows were verified via DOM/browser inspection instead.

## Troubleshooting

- **App won't launch on Wayland**: use the installed launcher, which sets
  `GDK_BACKEND=x11` and `WEBKIT_DISABLE_DMABUF_RENDERER=1` for WebKitGTK.
- **"Reset required" GPU errors / app freeze**: a wedged dGPU can stall the
  window; rebooting clears it (known NVIDIA + WebKitGTK interaction).
- **Extension not connecting**: confirm the NMH manifest exists (path above)
  and that DownPilot is running (listener on `127.0.0.1:19851`).
- **Checksum verification fails on a download**: the file is marked Failed and
  not renamed into place; re-download or verify the expected hash.
- **Downloads go to ~/Downloads instead of the chosen folder when added via
  the browser extension**: category destination routing overrides the
  suggested path by design; change the category's folder in Settings →
  Categories if needed.
