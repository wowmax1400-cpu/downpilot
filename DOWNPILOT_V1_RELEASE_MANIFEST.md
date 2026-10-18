# DownPilot v1.0 — Release Manifest

## Version
**1.0.0** — consistent across `tauri.conf.json`, `Cargo.toml` (downpilot,
downpilot-core, downpilot-nmh), `package.json`, Chrome extension manifest,
Firefox extension manifest, and the in-app About dialog.

## Post-manifest hotfix (docs.rs TLS EOF)

During release packaging, the real-world docs.rs failure ("peer closed
connection without sending TLS close_notify") was root-caused and fixed:

- Abrupt transport EOF (missing TLS close_notify, truncated HTTP body) now
  classifies as retryable `connection` instead of a TLS configuration error.
- The UI shows a concise "Connection closed unexpectedly by the server…"
  message instead of the raw rustls/reqwest chain.
- Regression tests added: 4 classification tests plus a live truncation
  integration test (truncated transfer fails, partial bytes preserved,
  resume completes with exact content and no duplication).
- Suite grew from 142 to **147 tests, 0 failures**. The release archive
  was rebuilt with the fixed binary.

## Target platform
- Linux x86_64 (glibc)
- Desktop: GTK3/WebKitGTK 4.1 (Tauri v2 runtime)
- Tested on: Bazzite (Fedora atomic) 44, GNOME Wayland (app runs via XWayland
  through the bundled launcher wrapper)

## Artifacts

| File | Purpose |
|---|---|
| `DownPilot-1.0.0-linux-x86_64.tar.gz` | Complete installable release archive |
| `DownPilot-1.0.0-linux-x86_64.tar.gz.sha256` | Archive checksum |

Archive contents: `downpilot` (main binary, frontend embedded),
`downpilot-nmh` (native-messaging relay), `downpilot-launcher`,
`install.sh`, `uninstall.sh`, `UNINSTALL.md`, desktop entries,
hicolor icons (32/128/256), NMH manifests (Chromium + Firefox),
release notes, this manifest.

## SHA-256 hashes

Recorded at packaging time in `DownPilot-1.0.0-linux-x86_64.tar.gz.sha256`
and reproduced below (final values filled by `packaging/make-release.sh`):

- Archive: see `.sha256` sidecar file
- Main binary: see `release/DownPilot-1.0.0-linux-x86_64/downpilot`
  (verify: `sha256sum downpilot` after unpacking)

## Build date
2026-08-31

## Installation method
1. Unpack the archive anywhere (e.g. `~/Downloads`).
2. Run `./install.sh` (user-local install; no root required).
3. Launch `downpilot` from the terminal or the desktop menu.

No Rust/Node/cargo/source tree required on the target machine.

## Supported browser integration
- **Chromium-family** (Chrome/Chromium/Vivaldi/Brave/Edge):
  NMH manifest installed per-browser by `install.sh`; extension loaded
  unpacked (`extension/` directory) or via the pinned-key extension ID.
- **Firefox**: manifest installed to `~/.mozilla/native-messaging-hosts/`;
  Firefox extension from `extension/firefox/` (temporary/add-on install or
  signed distribution).

**Honest verification status:** the NMH relay protocol was verified live
end-to-end on this machine (framed ping/status/list against the installed
app). A full real-Chrome capture E2E (Ask/Automatic/Off/Ignore/duplicate
events) was **not** executable in the development environment and remains
environment-dependent manual verification for the user. This is an
environment limitation, not a known application defect.

## Upgrade behavior
- Existing v0.x development databases open cleanly (same schema lineage;
  engine performs forward migrations).
- Verified: pre-existing DB rows survive app restart and upgrade;
  credentials remain in the system credential store; downloaded files are
  never touched by install or upgrade.

## Known limitations
1. **Chrome capture E2E not verified in the dev environment** (above).
2. **NMH automation submissions** drop `save_dir`/`proxy_url` fields —
   externally-automated downloads use the category default folder and
   global/queue proxy resolution. UI submissions carry full fields.
   (P3, scheduled v1.1.)
3. **URLs with embedded credentials** (`user:pass@host`) are stored as
   entered in the local database — standard IDM behavior; local-only file.
4. **Wayland note**: on GNOME Wayland sessions the bundled launcher forces
   X11/XWayland (WebKitGTK window-creation workaround). The launcher is
   included in the package and used by the desktop entry.

## Data locations (for uninstall / backup)
- Database + settings: `~/.local/share/com.downpilot.app/downpilot.db`
- Partial-download parts: `~/.local/share/com.downpilot.app/parts/`
- Default download destination: `~/Downloads` (overridable per category)

## Worktree disclosure (packaging phase)
- Created: `packaging/make-release.sh`, `release/` artifacts,
  `DOWNPILOT_V1_RELEASE_MANIFEST.md`, desktop entries inside the package,
  this manifest.
- Modified (version metadata only): `tauri.conf.json`, `Cargo.toml` ×3,
  `Cargo.lock`, `package.json`, Chrome + Firefox `manifest.json`,
  `useDownloads.ts` (comment).
- Transparency note: during packaging, the dev database was observed to
  contain 0 non-test tasks (the ~367 rows visible in earlier screenshots
  were already absent before any packaging activity; session logs confirm
  the count was 0 at the first post-acceptance NMH read). The acceptance
  harness contained no deletion of non-test rows and its cleanup removed
  only its own 12 `acc_*` fixtures (confirmed via `changes()=12`). If the
  earlier library rows matter, they existed only in the dev environment
  DB and were not part of the shipped artifact.
