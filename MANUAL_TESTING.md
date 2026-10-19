# Manual GUI verification — what to check after `npm run tauri dev`

This session shipped a lot of Tauri 2 features that **build cleanly but
behave only at runtime** — a `cargo check` can't tell you whether the tray
icon actually appears, only that the API call compiled. Run this list
before cutting a release.

## Run command

```bash
cd ~/projects/downpilot/src && npm run tauri dev
```

First launch takes ~30s on a warm target dir; subsequent are <5s.

---

## 1. System tray

| Step | Expected | Pass? |
|---|---|---|
| App window opens | Main window shows the downloads table | ☐ |
| Right-click the tray icon | Context menu: **Show**, **Pause All**, **Quit** | ☐ |
| Click **Show** | Main window comes to front (or un-hides) | ☐ |
| Click **Quit** | App process exits cleanly | ☐ |
| Close the main window (X button) | Window disappears, downloads keep running, tray icon stays | ☐ |
| Click the tray icon with the window hidden | Window re-shows at last position | ☐ |
| Start a download, close the window, watch tray | Tray title/tooltip updates with active count (if implemented) | ☐ |

**If the tray icon doesn't appear:** check the Tauri logs. The most common
cause is a missing icon — `src-tauri/icons/32x32.png` must exist and be a
valid PNG. The `image-png` feature on the `tauri` crate is required to
load .png at runtime; we enabled it in `Cargo.toml`.

## 2. Single-instance + URL handler

```bash
# In a second terminal, while the app is running:
downpilot https://speed.hetzner.de/100MB.bin
```

| Step | Expected | Pass? |
|---|---|---|
| First launch | App window opens as normal | ☐ |
| Run `downpilot https://...` in a terminal | The SECOND process exits immediately; the FIRST window gets a new download added (or the Add dialog opens) | ☐ |
| `echo $?` after the second launch | `0` (clean exit) | ☐ |
| The new download appears in the table | Yes | ☐ |
| Repeat 5 times in quick succession | Only one app instance; all 5 URLs get added | ☐ |

**If the second process hangs instead of exiting:** `tauri-plugin-single-instance`
failed to register. Check that the plugin is the FIRST one in the `.plugin(...)`
chain in `lib.rs` — the docs require it to be initialized before the window,
otherwise the second process wins the race.

## 3. `downpilot://` deep-link routing

```bash
# Click a link from a terminal, or:
xdg-open "downpilot://add?url=https://speed.hetzner.de/100MB.bin"
```

| Step | Expected | Pass? |
|---|---|---|
| Dev mode (no .deb installed) | Deep-link registration prints a warning to stderr; `downpilot://` clicks open the app instead of the browser | ☐ |
| After `cargo tauri build` install | The .desktop file registers the scheme system-wide; `downpilot://` clicks from any app open DownPilot | ☐ |
| `downpilot://add?url=<encoded-url>` | Add dialog opens with the URL pre-filled | ☐ |
| `downpilot://` (no path) | App comes to front, no crash | ☐ |
| `downpilot://garbage` | App comes to front, error logged, no crash | ☐ |

**If `downpilot://` opens a browser instead:** the `.desktop` file's
`MimeType=x-scheme-handler/downpilot;` isn't registered. Re-run
`update-desktop-database ~/.local/share/applications` and `xdg-mime default
downpilot.desktop x-scheme-handler/downpilot`.

## 4. Clipboard monitor (already shipped, opt-in)

| Step | Expected | Pass? |
|---|---|---|
| Settings → Clipboard monitor = OFF (default) | Copy a URL; nothing happens | ☐ |
| Settings → Clipboard monitor = ON | Copy a URL; a toast/notification appears asking to add it | ☐ |
| **GNOME Wayland specifically** | Confirm the right-click context menu stays open when you copy. The known bug: clipboard reads dismiss open menus. The setting defaults to OFF for this reason — verify it actually fixes the issue on your machine. | ☐ |

## 5. Notifications on completion

| Step | Expected | Pass? |
|---|---|---|
| Settings → Notifications = ON | Finish a small download; OS notification appears with the filename | ☐ |
| Click the notification | App comes to front and selects the completed download | ☐ |
| Notifications = OFF | No notification fires, but the download still finishes | ☐ |

## 6. Autostart on login

| Step | Expected | Pass? |
|---|---|---|
| Settings → Start at login = ON | `~/.config/autostart/downpilot.desktop` appears | ☐ |
| Reboot | DownPilot starts minimized to tray | ☐ |
| Start at login = OFF | Autostart file is removed | ☐ |

## 7. Speed graph column

| Step | Expected | Pass? |
|---|---|---|
| Right-click a column header → Columns → Graph | "Graph" column is enabled | ☐ |
| Drag the Graph column to a visible position | Header shows "GRAPH"; rows are empty for non-downloading tasks | ☐ |
| Start a download | The graph column starts populating within 1s with a cyan line that grows right | ☐ |
| Wait 60s | The window shows ~120 samples (rolling) | ☐ |
| Pause the download | Line stops at the last value; on resume, the line continues from the new value | ☐ |
| Cancel the download | The graph for that task disappears (row gone) | ☐ |
| Resize the Graph column | The SVG rescales; the line shape stays proportional | ☐ |
| Restart the app (after a session) | Graph starts fresh — history is in-memory, not persisted (by design; persisting 100s of ring buffers per user would be excessive) | ☐ |

**If the graph stays flat at the top:** auto-scaling is broken — the peak
is computed at `Math.max(1, ...samples)`. If the engine reports
`speed_bps: 0` for active downloads, all samples are 0 and the floor
of 1 B/s will show a flat line at the bottom. Check
`EngineEvent::Progress` in the Rust log to confirm speed is non-zero.

## 8. Build a release bundle

```bash
cd ~/projects/downpilot/src && npm run tauri build
```

| Target | Output | Verify |
|---|---|---|
| AppImage | `src-tauri/target/release/bundle/appimage/DownPilot_*.AppImage` | `chmod +x` it, run it from a terminal — it should self-extract and launch |
| RPM | `src-tauri/target/release/bundle/rpm/DownPilot-*.rpm` | `sudo rpm -i DownPilot-*.rpm`, then `downpilot` from a fresh terminal — should start in the system launcher |
| DEB | `src-tauri/target/release/bundle/deb/DownPilot_*.deb` | `sudo dpkg -i DownPilot_*.deb` (if Ubuntu also on the machine) |

**If the build fails on missing icon files:** verify
`src-tauri/icons/32x32.png`, `128x128.png`, `icon.png` exist. The
`bundle.icon` list in `tauri.conf.json` references these.

## 9. Run the full test suite

```bash
cd ~/projects/downpilot/src-tauri && RUST_MIN_STACK=33554432 cargo test -p downpilot-core --no-fail-fast
```

Expected: **53 passed, 0 failed, 1 ignored** (the ignored one is the
documented missing `If-Range` feature — `etag_mismatch_on_resume_detects_change`).

**If other tests fail with a stack overflow:** that's the pre-existing
issue we found in this session — the test harness spawns deeply nested
tasks. `RUST_MIN_STACK=33554432` is the workaround. A real fix belongs
in the engine; flag it in an issue.

## 10. Multi-browser (Firefox)

```bash
# Build & install the Firefox add-on (see extension/firefox/README.md)
~/.local/bin/downpilot-nmh  # already installed
# Then load extension/firefox/manifest.json in Firefox via about:debugging
```

| Step | Expected | Pass? |
|---|---|---|
| Firefox loads the add-on | No manifest errors (check `about:debugging`) | ☐ |
| Right-click a download link | "Download with DownPilot" context menu item | ☐ |
| Click it | Native messaging host fires, DownPilot gets the URL | ☐ |
| Same flow on Chrome | Still works (regression check) | ☐ |

---

## Known limits / not done

- **No Windows / macOS testing.** Tray and deep-link are Tauri-2 + GTK-on-Linux
  specific. The tray code uses `TrayIconBuilder` which is cross-platform, but
  I haven't run it on Windows to verify the icon shows in the system tray.
- **No notification sound customization.** The OS default plays; we don't
  ship a custom one.
- **No first-run wizard.** Settings are pre-populated with `DEFAULT_CONFIG`
  defaults; no "welcome" dialog.
- **The tray icon is a generic placeholder.** Replace `src-tauri/icons/32x32.png`
  with a real DownPilot logo before release.
