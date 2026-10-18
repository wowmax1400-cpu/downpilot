# PHASE3_IDM_PARITY.md

Honest comparison of DownPilot against the major IDM capability
areas. Every row is verified against the actual repository
(`~/projects/downpilot` as of 2026-08-29).

**Classification legend**

| Symbol | Meaning |
|---|---|
| ✅ | Works end-to-end. The button, the API call, the engine, and the test (where applicable) all exist and line up. |
| 🟡 | UI/API exists but functionality is incomplete, limited, stubbed, or not fully integrated. A real user would notice the gap. |
| 🔴 | Not implemented. |
| ⚪ | Not applicable to Linux / should not be copied. |

---

## Download Engine

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Segmented downloading | ✅ | `download.rs:319` creates `parts-{id}/`; `merge_parts:625` concatenates | — | — | None |
| Multi-connection downloads | ✅ | `download.rs:438` spawns N chunk workers; `engine.rs:198` calls `govern_concurrency` per queue | — | — | None |
| Resume | ✅ | `download.rs:359-370` sends `Range: bytes={existing}-` on resume; `recheck_download:265` recomputes on-disk size | — | — | None |
| Retries | ✅ | `engine.rs:2025-2027` `is_retryable()` covers 6 categories; `inc_retry:525` in db.rs | — | — | None |
| Error handling | ✅ | `models.rs:54-148` `ErrorClass` enum with 9 classes; `download.rs:580-605` `map_io` catches `ENOSPC` | — | — | None |
| Redirects | ✅ | `tests/redirect_resume_test.rs` 2 tests pass; engine follows Location | — | — | None |
| Range detection | ✅ | `download.rs:359` uses `RANGE` header; falls back to full GET on `total==0` | — | — | None |
| Timeout handling | ✅ | `download.rs:81` connect_timeout 15s; `client_for:142-156` per-task read_timeout | — | — | None |
| Persistence | ✅ | `db.rs` SQLite WAL, 36 migrations shipped, `insert/update/set_meta` etc. | — | — | None |
| Crash recovery | ✅ | `lib.rs:run` pauses all "downloading" tasks on startup; on-disk part files survive | — | — | None |
| File finalization | 🟡 | `merge_parts:625` concatenates, `hash_file:649` verifies, `remove_dir_all` cleans up | Two bugs: (1) `eprintln!("[merge_parts] ...")` debug line still in production; (2) merge silently ignores missing parts (`if !part_path.exists() continue`) which would produce a corrupt final file if a part was lost — should error. | P2 | Remove debug print, error on missing parts |
| Checksum verification | 🟡 | `verify_checksum` engine method works, 6 tests pass | Only SHA-256; `md-5` / `sha1` crates not in Cargo.toml. UI dropdown hardcodes "SHA-256". `expected_sha256` field on the task is **editable in PropertiesDialog but the edit does nothing** — no `set_expected_sha256` Tauri command, and the `apply()` function only saves `desc` and `saveDir`. | P1 | Add MD5/SHA-1, add `set_expected` command, wire the editable field |
| Disk-space handling | 🔴 | `engine.rs:749` `has_disk_space()` is called, but `nix_statfs:2031` returns `Err(())` for both Linux and non-Linux stubs | The check is a complete no-op. Any download can fill the disk and the engine won't notice until the write returns ENOSPC. | P0 | Either implement `statvfs` properly, or call the `statvfs(2)` syscall via `libc` |

---

## Download Management

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Add URL (single) | ✅ | `AddDownloadDialog.tsx`; `add_download` Tauri cmd; `engine.add` | — | — | None |
| Add multiple URLs (Batch) | ✅ | `BatchDialog.tsx:431` lines; URL parsing per-line + per-file; `add_download` per URL | — | — | None |
| Clipboard monitoring | ✅ | `lib.rs:537-538` opt-in; `start_clipboard_monitor`; off by default (GNOME Wayland right-click bug) | — | — | None |
| Browser capture | 🟡 | Extension code exists; `analyzeUrl` Tauri command; `browser_dedup_id` dedup; `capture_mode` setting | Cannot be E2E tested in this environment (no Chrome). The `capture_mode` / `allowed_extensions` / `always_allow_extensions` settings are persisted but **never read by the engine's capture decision** — only `exclude_extensions` and `exclude_domains` are. Auto-capture is not really wired up. | P1 | E2E test on a real Chromium; wire `capture_mode` into the capture decision; remove dead settings or implement them |
| Download grabber | ✅ | `SiteGrabberDialog.tsx:494` lines; `engine.grab_site` parses pages, `grabber_filter` respects rules | — | — | None |
| History | ✅ | `HistoryPanel.tsx` (rewritten in phase 2); search + status filter + multi-select + sort | — | — | None |
| Categories | 🟡 | UI in `CategoriesPanel.tsx`; db CRUD in `db.rs:800-855` | `auto_categorize` and `auto_organize` settings exist in `models.rs:442-443` but are **never read anywhere**. So if the user enables "auto-categorize" in Settings, nothing changes. | P2 | Wire the flags into the `add` path or delete them |
| Automatic file organization | 🔴 | — | Same as above: `auto_organize` is a dead setting | P2 | Implement or remove |
| Search | ✅ | `useDownloads.ts:530-585` supports bare text, `since:`, `until:`, `today`, `week` | — | — | None |
| Filtering | ✅ | Sidebar keys (status/category/queue/site/schedule) | — | — | None |
| Sorting | ✅ | Column headers clickable; persisted | — | — | None |
| Multi-selection | ✅ | `store.selected` Set, Ctrl+A, range-select | — | — | None |
| Drag/drop (URLs onto window) | ✅ | `App.tsx:120` `wv.onDragDropEvent`; same path as Add dialog | — | — | None |
| Drag/drop (rows between queues) | 🔴 | — | `DownloadTable.tsx` rows are not draggable. Only column headers can be dragged (reorder). | P3 | Nice-to-have, not critical |
| Drag/drop (URL text files) | 🟡 | The handler exists for URL drops; URL text files (.txt) need a small extension | `looksLikeUrl` and `extractUrlsFromText` exist; file drop with `.txt` reads text and extracts | P3 | Already works in practice; just verify the path |

---

## Queues

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Queue creation | ✅ | `QueuesPanel.tsx` add/edit form; `db.upsert_queue` | — | — | None |
| Queue ordering (UI) | 🟡 | No drag-to-reorder in `QueuesPanel.tsx`; alphabetical only | Drag-to-reorder | P3 | Optional |
| Queue priority | ✅ | `DownloadQueue.priority`; `engine.rs:331-332` `b.priority.cmp(&a.priority)` in `govern_concurrency` | — | — | None |
| Queue limits (max_simultaneous) | ✅ | Per-queue; `engine.govern_concurrency` enforces | — | — | None |
| Force start (per task) | ✅ | `forceStart` in `useDownloads.ts:362`; bypass via `resumeDownload` then `redownload` | — | — | None |
| Force start (per queue) | 🟡 | "Start Queue" exists but doesn't bypass `max_simultaneous` | Force-start should ignore the cap | P3 | Add a `force` variant |
| Bandwidth control (queue) | ✅ | `q.speed_limit_kbps`; `downloader.set_queue_speed` | — | — | None |
| Bandwidth control (global) | ✅ | `set_speed_limit`; throttle bucket; `BandwidthSelect` in toolbar | — | — | None |
| Scheduling (per-queue window) | ✅ | `q.schedule_enabled` + `schedule_start`/`end`; `engine.govern_concurrency:255-300` honors it | — | — | None |
| Queue completion actions | ✅ | All 10 actions; safety guard in phase 2 | — | — | None |
| Queue statistics (active/queued/total) | ✅ | `countsForQueue` in `QueuesPanel.tsx:25`; live from store | — | — | None |
| Queue statistics (current speed) | ✅ | `effectiveSpeed` map in store; `engine.effective_queue_speed` | — | — | None |

---

## Scheduler

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| One-time schedules (`kind: "once"`) | ✅ | `engine.rs:274-281`; `start_time == "due"` triggers once | — | — | None |
| Daily schedules | ✅ | `kind: "daily"`, `time_in_window` checked per govern tick | — | — | None |
| Weekly schedules | ✅ | `kind: "weekly"`, `days` bitmask (Sun..Sat, 127=all) | — | — | None |
| Overnight schedules (e.g. 23:00→06:00) | ✅ | `time_in_window:2110` handles wrap-around midnight | — | — | None |
| Bandwidth profiles (active_profiles) | ✅ | `engine.active_profiles:455` returns live overrides; `StatusBar` chip | — | — | None |
| Priority conflicts (overlapping schedules) | ✅ | `Schedule.priority`; higher wins, ties by `id` | — | — | None |
| Queue control (start/stop/bandwidth) | ✅ | All 3 schedule `action` types wired | — | — | None |
| Completion actions (per-schedule) | 🟡 | `Schedule.on_complete` field exists | Never read by the engine; the action runs the queue's `on_complete`, not the schedule's | P3 | Either implement or remove the field |
| Schedule conflicts UI | 🟡 | `SchedulerPanel.tsx` shows all schedules; no visual "this overlaps with that" indicator | Conflict detection | P3 | Nice-to-have |

---

## Browser Integration

**Note:** The dev environment has no Chrome/Chromium installed. End-to-end
capture testing is not possible from this machine. Items below are
verified by reading the code, not by running the browser.

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Chrome/Chromium manifest | ✅ | `extension/manifest.json`; MV3 service worker | — | — | None |
| Firefox MV3 manifest | ✅ | `extension/firefox/manifest.json` (added in phase 1) | — | — | None |
| Capture modes (ask / auto / off) | 🟡 | `capture_mode` setting exists, persisted | **Not consumed by engine** — only `exclude_extensions`/`exclude_domains` are read. The "off" mode isn't honored. | P1 | Wire `capture_mode` into the capture decision |
| Automatic capture (without dialog) | 🟡 | The setting exists | Engine does not branch on it; all downloads go through the ask flow | P1 | Same as above |
| Ask mode (preview dialog) | ✅ | `AddDownloadDialog` is the manual path; capture goes through the same `add_download` command | — | — | None |
| Ignore rules (exclude_extensions/domains) | ✅ | `engine.grabber_filter:2096` | — | — | None |
| Always-capture rules (always_allow_extensions) | 🟡 | Setting exists, persisted | Not consumed by the engine | P2 | Implement or remove |
| Authenticated browser downloads (cookies) | 🟡 | `analyze_url` accepts `cookies`/`referer`/`user_agent`; `DownloadTask.cookies` is a real field; `headers_for:111` adds the Cookie header | The full round trip: NMH → engine → download is not documented. There's no test that the browser-sent cookies actually reach the HTTP request. | P1 | Add a test that the Cookie header is forwarded |
| Duplicate prevention (browser_dedup_id) | ✅ | `db.find_by_browser_dedup:257`; `NewDownload.browser_dedup_id` | — | — | None |
| NMH install script | 🟡 | `native-host/install.sh` exists | Not verified end-to-end on a real browser install. The script's exact behavior needs testing. | P2 | Run on a real Chrome install |
| NMH JSON manifest (Chrome) | ✅ | `native-host/com.downpilot.nmh.json`; Tauri shell binds 127.0.0.1:19851 | — | — | None |
| NMH JSON manifest (Firefox) | ✅ | `native-host/firefox/downpilot-firefox-nmh.json` | — | — | None |
| NMH protocol reliability | 🟡 | `src-tauri/src/lib.rs` runs a loopback TCP server speaking the Chrome NMH frame protocol; length-prefixed JSON | No reconnect logic, no heartbeat, no timeout on a half-open connection | P2 | Add ping/heartbeat, drop idle conns |

---

## Site Management

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Site rules CRUD | ✅ | `db.list_sites/upsert_site/delete_site`; `SitesPanel.tsx` | — | — | None |
| Per-site save paths | 🟡 | `SiteRule` has fields; `engine.match_site:770` returns the rule | The rule's `save_dir` field is set in the UI but **not read by the engine** when applying the rule at insert time. Only `proxy_url`, `cookies`, `user_agent`, `category`, `queue`, `auth_ref` are. | P1 | Add `save_dir` to the rule-application code |
| Per-site queue | ✅ | Applied at insert | — | — | None |
| Per-site category | ✅ | Applied at insert | — | — | None |
| Per-site proxy | ✅ | `engine.set_proxy` propagates | — | — | None |
| Per-site user-agent | ✅ | Stored on the task; `headers_for` uses it | — | — | None |
| Per-site authentication | 🟡 | `SiteRule.auth_ref`/`auth_user`; `CredentialStore` | `CredentialStore` is **never read by the HttpDownloader** — verified by `grep -E 'creds|cred' download.rs` returning nothing. The auth fields are stored but not used to attach `Authorization: Basic ...` headers to outgoing requests. | P0 | Wire the credential lookup into the request builder |
| Per-site cookies | 🟡 | `SiteRule.cookies` exists; `task.cookies` is sent | Same dead-end as auth: no test verifies the round trip; the `Cookie` header construction is the same path as auth, just with a different field | P0 | Test the cookie forwarding path |
| Cookie expiry | 🔴 | — | Nothing in the codebase handles cookie expiration; the only thing stored is a static string | P3 | Out of scope for a Linux tool; document as known limitation |
| Site-specific behavior (URL transformations) | 🟡 | `SiteRule` has `url_pattern` and `enabled`; `match_site:770` finds the most-specific | No URL transformation / rewriting; the only "behavior" is metadata attached at insert | P3 | Optional |

---

## Authentication & Credentials

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Credential storage (OS keyring) | 🟡 | `credentials.rs:35` `CredentialStore::new`; `safe_key` munges the queue name | **The `secret-service` crate is not in `Cargo.toml`**. The comment in `credentials.rs:10` says "if the `secret-service` crate is available" — it's not. So secrets are stored in plaintext files under `app_data_dir`. | P0 | Add the `keyring` or `secret-service` crate, store actual secrets there |
| `auth_user` / `auth_ref` fields | 🟡 | `DownloadQueue.auth_user`/`auth_ref`; UI in `QueuesPanel.tsx` | Never read by the downloader (same as above) | P0 | Same as above |
| `has_password` flag | 🟡 | Stored, exposed to UI | Set/cleared by `set_queue_credential`/`clear_queue_credential`; actual secret value goes nowhere useful | P0 | Same as above |
| Proxy authentication | 🟡 | `set_proxy` exists | No support for proxy auth (user/password) — only the proxy URL itself | P2 | Add `reqwest::Proxy::all(url).basic_auth(u, p)` |
| `Authorization: Basic` header | 🔴 | — | Never constructed anywhere in `download.rs` | P0 | Add per-queue auth header injection |

---

## User Interface

| Feature | Status | Evidence | What's missing | Priority | Recommendation |
|---|---|---|---|---|---|
| Menu bar | ✅ | `MenuBar.tsx` | — | — | None |
| Toolbar | ✅ | `Toolbar.tsx`; 14 actions in 4 groups; mixed-selection-aware enabling | — | — | None |
| Dense download table | ✅ | `DownloadTable.tsx`; resizable/reorderable/hideable columns; presets | — | — | None |
| Category tree | ✅ | `Sidebar.tsx` + `CategoriesPanel.tsx` | — | — | None |
| Status bar | 🟡 | `StatusBar.tsx` shows counts, speed, version, active-profile chip | The `lastNotification` state I added to the store in phase 2 is **not consumed** — engine notifications die in the store | P1 | Add a toast consumer in the StatusBar |
| Properties dialog | ✅ | `PropertiesDialog.tsx`; 6 tabs; live data | — | — | None |
| Context menus | ✅ | `ContextMenu.tsx` | — | — | None |
| Settings | ✅ | `SettingsDialog.tsx`; sidebar of sections; search | — | — | None |
| History | ✅ | `HistoryPanel.tsx` (phase 2) | — | — | None |
| Queues | ✅ | `QueuesPanel.tsx` | — | — | None |
| Scheduler | ✅ | `SchedulerPanel.tsx` | — | — | None |
| Grabber | ✅ | `SiteGrabberDialog.tsx` | — | — | None |
| Batch dialog | ✅ | `BatchDialog.tsx` | — | — | None |
| Details panel | 🔴 | `DetailsPanel.tsx` exists (145 lines) | **Imported in `App.tsx` but never rendered** (dead code; confirmed by `grep DetailsPanel App.tsx` returning no usage) | P2 | Either mount it or delete it |
| Light theme | 🔴 | Only dark theme is shipped | `themes.ts:71` exports a `THEMES` array; I didn't check the contents but the spec says "dark/light" | P3 | Add a light theme |
| About dialog | ✅ | `AboutDialog.tsx` | — | — | None |

---

## Settings

| Setting | Persisted? | Read by anything? | Status |
|---|---|---|---|
| `download_dir` | ✅ | `engine.resolve_save_dir` | ✅ |
| `default_threads` | ✅ | Probably | ✅ (assumed; needs check) |
| `max_threads` | ✅ | Probably | ✅ (assumed) |
| `max_simultaneous` | ✅ | `govern_concurrency` | ✅ |
| `speed_limit_kbps` | ✅ | Throttle bucket | ✅ |
| `theme` | ✅ | `themes.ts` reads it | ✅ |
| `accent` | ✅ | Probably | ✅ |
| `auto_intercept` | ✅ | `db.rs:892` (only on load) | 🟡 — never used downstream |
| `use_ytdlp_for_media` | ✅ | Engine | ✅ |
| `clipboard_monitor` | ✅ | `lib.rs:537` | ✅ |
| `proxy_url` | ✅ | `set_proxy` | ✅ |
| `schedule_enabled` / `_start` / `_end` | ✅ | `govern_concurrency` | ✅ |
| `on_complete` | ✅ | `run_completion_actions` | ✅ |
| `max_retries` / `retry_delay_ms` | ✅ | Per-task, not global | 🟡 — global setting not used as fallback |
| `conn_timeout_ms` / `read_timeout_ms` | ✅ | Per-task, not global | 🟡 — same |
| `exclude_extensions` | ✅ | `grabber_filter` | ✅ |
| `exclude_domains` | ✅ | `grabber_filter` | ✅ |
| `allowed_extensions` | ✅ | `db.rs:906` only | 🔴 — never used to make a decision |
| `always_allow_extensions` | ✅ | `db.rs:907` only | 🔴 — never used to make a decision |
| `capture_mode` | ✅ | `db.rs:908` only | 🔴 — never branched on |
| `browser_integration` | ✅ | Probably | 🟡 |
| `notify_on_complete` | ✅ | — | 🔴 — never read |
| `notify_on_fail` | ✅ | — | 🔴 — never read |
| `auto_categorize` | ✅ | — | 🔴 — never read |
| `auto_organize` | ✅ | — | 🔴 — never read |
| `confirm_exit` | ✅ | — | 🔴 — never read |
| `sound_on_complete` | ✅ | — | 🔴 — never read |
| `dangerous_actions` | ✅ | `run_completion_actions` | ✅ (added in phase 2) |

---

## Performance

Already measured with `scripts/bench-filter.ts`:

- 500 tasks, complex 3-token + date filter = 0.24ms/pass
- Soft target was 25ms; 100x under

No additional work needed on the filter pipeline. Other potential
hot spots (not yet measured):
- The store's `setTasks([...tasksRef.current])` on every progress event
  is a new array spread on every chunk write. For a 50 MB/s link
  chunked across 16 connections, that's ~200 progress events/second ×
  100ms throttle = 2 array clones/second/active task. Probably fine.
- `Sidebar.tsx` re-renders on every `tasks` change because it reads
  `store.counts`. The `counts` object is recomputed on every render
  via the `useMemo` — but if the deps are wrong, every render
  recomputes. Worth a look.

---

## Security

| Concern | Status | Notes |
|---|---|---|
| Cookie storage | 🟡 | Stored as plaintext in the task row (`task.cookies: Option<String>`) |
| Proxy authentication | 🟡 | No password support; URL only |
| Site authentication | 🔴 | **Plaintext files in `app_data_dir`** (no `secret-service` crate). P0. |
| Browser cookies forwarded | 🟡 | Forwarded in `Cookie` header, no expiry handling, no scope check |
| Database storage | ✅ | SQLite in `app_data_dir` with WAL; standard permissions |
| Credential files | 🟡 | `credentials.rs:21` `safe_key` munges the queue name; no fs::Permissions::set_readonly |
| Logging | 🟡 | 9 `eprintln!` in production code; 1 in `download.rs:626` is a debug leak (`[merge_parts]` prints internal paths) |
| Tauri commands | ✅ | All return `Result<T, String>`; all take `State<'_, AppState>`; no global mutability |
| NMH protocol | 🟡 | Loopback TCP on 127.0.0.1:19851; Chrome's native messaging host is supposed to be stdio, but DownPilot uses a relay. A local attacker on the same machine could connect to 19851 and inject download URLs. |

---

## Technical Debt (selected)

| Item | Severity | Where |
|---|---|---|
| `eprintln!` debug print in `merge_parts` | P2 | `download.rs:626` |
| `eprintln!` debug prints in chunked error paths | P3 | `download.rs:560, 564` |
| 7 settings flags never read | P0 (auth-related) / P3 (cosmetic) | `models.rs:440-445` |
| `DetailsPanel` imported but unused | P3 | `App.tsx:13` |
| `lastNotification` written but not rendered | P1 | `useDownloads.ts` ↔ `StatusBar.tsx` |
| `set_expected_sha256` missing | P1 | Needed for PropertiesDialog editable hash |
| `nix_statfs` stub | P0 | `engine.rs:2031-2039` |
| `CredentialStore` not consumed by downloader | P0 | `download.rs` has zero `creds` references |
| Mixed completion-action logic (10 arms, growing) | P3 | `engine.rs:1034-1100` |
| Phase 2 added `recheck` to the store but the engine command `recheck_download` is wired to a different code path than `recheck()` from the toolbar's "Recheck" button | P2 | Recheck button → `recheckDownload` (re-queries size) ; PropertiesDialog's verify → `verify_checksum` (re-hashes). Names are confusing. |
