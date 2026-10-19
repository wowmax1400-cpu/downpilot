# DownPilot Roadmap

Where the app is strong today: chunked HTTP engine, pause/resume, SQLite
persistence, queues, scheduler, bandwidth profiles, site rules, site grabber,
browser bridge, per-queue auth (keyring-backed), theming.

Where it's weakest: desktop integration, protocol breadth, distribution.
That's what this roadmap orders.

---

## Phase 1 — Feel like a real desktop app (integration)

The features below are what make users stop thinking about the app.

1. **System tray** (highest priority)
   - Minimize/close to tray, never actually quit unless told.
   - Tray icon states: idle / downloading (animated) / error.
   - Quick menu: global pause/resume-all, current speed, recent completions.
   - This is the #1 gap vs IDM: with no tray, closing the window kills your
     downloads.
2. **Clipboard monitoring**
   - Watch for copied URLs (opt-in toggle), raise a small "Add download?"
     toast instead of a full dialog.
   - IDM's single most-loved convenience; cheap to build (Tauri clipboard
     events + URL regex).
3. **Desktop notifications + notification actions**
   - Completion/failure notifications; click-to-open-folder.
   - Needs notification support in the Rust shell (tauri-plugin-notification).
4. **Single-instance + URL handling**
   - `downpilot <url>` from any terminal should join the running app
     (single-instance plugin) and add the download.
   - Register as a handler for http/https in the desktop environment
     (`.desktop` file `MimeType`/`Exec` — matters for Bazzite/GNOME).
5. **`cargo tauri build` finishing line**
   - AppImage + RPM bundles that actually build, icon everywhere, `.desktop`
     file, `xdg-mime` registration.
   - Until this lands, the app is dev-mode-only and can't be shown to anyone.

## Phase 2 — Protocol & engine breadth

1. **Redirect-chain + referer hardening** — real-world CDNs (especially Iranian
   hosts behind Cloudflare) redirect mid-download; resume must replay the
   whole chain, not just the final URL. Already partially handled; add tests.
2. **Cookies from browser** — extension already talks to Chrome; add "import
   cookies for this download" so authenticated file hosts work without
   re-login. Biggest user-visible unlock for the extension.
3. **FTP (basic)** — old but still requested for archive hosts; ~200 lines
   on top of tokio.
4. **Segment reconciliation / ETag-If-Range**
   - If a file changed between resume attempts, detect via ETag and restart
     instead of producing a corrupt merge. Currently resume trusts part-file
     offsets blindly.
5. **Speed graph + per-connection detail**
   - IDM's 8-connection grid view; helps debugging slow hosts. UI-side
     mostly, engine already exposes per-chunk progress.
6. **Torrent (stretch goal, later)** — heavy dep (librqbit), only after
   Phase 1–2 are stable. Don't let it distract.

## Phase 3 — Browser extension maturity

1. **Interception rules UI in the app** (Sites panel extension): file-type
   filters (e.g. always take .iso/.zip > 50MB, never take .pdf), size
   threshold, per-domain allow/deny. IDM's "Options → Sites" is exactly this.
2. **Firefox port** — same MV3 code, new native-messaging manifest; opens the
   largest Linux user base (most Bazzite users default to Firefox).
3. **Mirror/multi-source** — for popular files, try fetching different byte
   ranges from mirror URLs; rarely needed but a differentiator.
4. **Popup QoL** — show live app speed + task count in popup, deep-link to
   the app window (needs single-instance from Phase 1).

## Phase 4 — Polish & distribution

1. **Flatpak** — the Bazzite-native distribution story (flathub submission
   later; personal repo first). Native messaging inside flatpak needs the
   `--filesystem=xdg-config/...` override pattern already proven with Chrome.
2. **Auto-update** — AppImageUpdate or tauri-plugin-updater; the app must
   not need manual rebuilds once friends use it.
3. **i18n** — Persian (fa) UI translation; trivial win for you personally.
4. **Backup/restore settings + task list** — export/import already exists
   for tasks; add settings + site rules to it.
5. **First-run wizard** — pick download dir, default threads, install
   extension hint, clipboard-monitor toggle.

## Deliberately NOT on the roadmap

- Torrenting as a day-one feature (scope trap until core feels perfect).
- Any macOS/Windows port — Linux-first is the identity.
- Plugin system — over-engineering for a personal app.

## Suggested order of attack

```
Tray  →  single-instance + URL handling  →  clipboard monitor
→  bundles/AppImage  →  browser-cookie imports  →  Firefox port
→  everything else as needed
```

Phases 1's first three items are small (a few days each) and change daily
usability the most. Do them before anything clever.
