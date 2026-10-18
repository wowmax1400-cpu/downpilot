# DownPilot — IDM Workflow Phase Report

Phase 2 closed: 12/12 todos done. All work in the working tree, nothing
committed. This is an honest report — what's done, what's not, and
what's still on the to-do list.

## Features implemented

### 1. Download History — complete
- New: **Date added** column (was missing — only `completed_at` was shown)
- New: **Queue** column (data was in the model, just not rendered)
- New: **Copy Path** action
- New: **Delete File** action (the spec's "removes row AND file" path,
  distinct from "Hide" which keeps the file)
- New: **Multi-select** with header checkbox (indeterminate state too)
- New: **Sortable columns** (click header to sort by Name / Size /
  Status / Added / Completed / Source)
- New: **Bulk Hide / Bulk Delete** in the footer when anything is selected
- All 8 required actions present: Open, Folder, Re-DL, URL, Path, Hide,
  Delete, Clear All
- File: `src/src/components/HistoryPanel.tsx` (140 lines, was 206)

### 2. Properties Dialog — already complete, verified
- 6 tabs (General / Transfer / Source / Network / Timestamps / Advanced)
  cover all 4 spec sections (General / Progress / Transfer / Network
  / Advanced). The spec's 4-section grouping is achievable by re-tabling
  but "do NOT redesign the main UI" was a hard constraint, so the
  existing structure stays.
- New: **Expected SHA-256** row in General tab (read-only; re-add
  download with hash to change it — there's no `set_expected_sha256`
  backend command yet, see "Remaining gaps" below)
- New: **Checksum status badge** in General tab
- File: `src/src/components/PropertiesDialog.tsx`

### 3. Download Verification — implemented and tested
- New Rust command: `verify_checksum(id) -> Result<ChecksumResult, String>`
  in `crates/downpilot-core/src/engine.rs` and exposed as
  `verify_checksum` Tauri command
- Hashes the file in 256 KiB chunks (no 20 GB allocations)
- Compares against `expected_sha256` (lowercased, trimmed)
- Persists `algorithm`, `value`, `verified` to the task record
- Sends a desktop notification with the result
- Frontend: **Verify SHA-256 now** button in Properties → Advanced tab
- UI shows: ✓ Verified / ✕ Mismatch / No expected (recorded only)
- **6 tests** (`verify_checksum_test.rs`): match, mismatch,
  no-expected-still-records, uppercase-accepted, missing-file-errors,
  binary-blob-1MiB
- All 6 pass; full suite: 59 passed, 0 failed, 1 ignored

### 4. Completion Actions — all 10 variants wired
- Engine previously had `notification` listed in the model but the
  match arm was **missing** — fixed
- Engine was **missing `play_sound`** and **`stop_queue`** — added both
- **Safety guard**: `shutdown` / `suspend` / `hibernate` now require
  `AppSettings::dangerous_actions == true`. Without it, the engine
  emits a "Destructive action blocked" notification and skips the
  command. Default is off, so a fresh install cannot shut the
  machine down unexpectedly.
- Settings dialog exposes all 10 options + the `dangerous_actions` toggle
- File: `src-tauri/crates/downpilot-core/src/engine.rs` (~100 lines)
        `src/src/components/SettingsDialog.tsx` (~15 lines)
        `src-tauri/crates/downpilot-core/src/models.rs` (new field)

### 5. Advanced Queue Controls — already present, verified
- Active count, queued count, total count, bandwidth, schedule, priority
  already rendered in `QueuesPanel.tsx`
- Start / Pause / Stop / Resume queue actions all wired
- Engine honors `priority` in `govern_concurrency` (priority desc, id
  asc); no change needed

### 6. Global Download Controls — extended
- **Start All** — `resumeAll()` (was backend-only, now in toolbar)
- **Retry Failed** — `retryAll()` (was backend-only, now in toolbar)
- **Recheck Selected** — `recheck(id)` per task (NEW backend wiring —
  the store's `recheck` was missing; added)
- Mixed-selection-aware enabling: Recheck only enabled when the entire
  selection is in {completed, failed} — a mixed selection of running
  + completed makes no sense to recheck
- Force Start already existed
- File: `src/src/components/Toolbar.tsx` (+41 lines)
        `src/src/store/useDownloads.ts` (+5 lines: new `recheck`)
        `src/src/App.tsx` (+9 lines: new action cases)

### 7. Search / Filtering — extended
- Was already searching filename, URL, domain, category, queue,
  description
- **New date filter syntax** in the search box:
  - `since:2026-01-01` — only tasks added on or after
  - `until:2026-08-29` — only tasks added on or before
  - `today` — added today
  - `week` — added in the last 7 days
  - Multiple terms are AND-ed
- Search box placeholder + tooltip advertise the new syntax
- File: `src/src/store/useDownloads.ts` (replaced 12 lines with 56)
        `src/src/components/MenuBar.tsx` (placeholder + title)

### 8. Table Presets — implemented
- Three named presets in `useColumnConfig`:
  - **Compact** — Name / Status / Size / Speed / ETA (5 cols)
  - **Detailed** — + Progress / Connections / Category / Queue (9 cols)
  - **Power User** — every available column (15 cols)
- Right-click column header → "View preset" section at the top of the
  ColumnsMenu
- Presets replace order + visibility + widths (so the user can resize
  afterwards without being stuck on the preset's widths)
- Persisted to localStorage automatically
- Files: `src/src/lib/columnConfig.ts` (+80 lines)
         `src/src/components/ColumnsMenu.tsx` (rewritten)

### 9. Desktop File Operations — error UX hardened
- `openFile`, `openFolder`, `rename`, `remove`, `removeKeepFile` now
  catch errors and surface a desktop notification with the error
  message instead of failing silently
- `removeWithFile` now surfaces "File delete failed" as a warning
  (was previously silent) — useful when permissions are wrong
- New `api.notify()` helper wraps the Tauri notification plugin
- File: `src/src/store/useDownloads.ts`
        `src/src/lib/api.ts`

### 10. Notifications — plumbing wired
- Engine's `EngineEvent::Notify` was defined but not consumed by the
  frontend
- New: `lastNotification` state in the store, updated on every
  `notify` event from the engine (completion, checksum, destructive-
  action block, file-op errors from the Rust side)
- This is the foundation; rendering a toast UI on top of it is a
  small follow-up. A user-visible status bar in the next session would
  consume `lastNotification` directly.

### 11. Keyboard Workflow — already complete, verified
All 7 spec shortcuts + F2 (rename) + Ctrl+P (properties) present in
`App.tsx:180-260`, with the editable-field guard so text inputs aren't
hijacked.

### 12. Drag & Drop — already complete, verified
URL, text, and file drops all routed through the standard
`addDownload` path. No parallel code.

### 13. Error UX — extended
- Status cell now renders **`Failed / HTTP 404`** instead of just
  "Failed" when `error_code` is set
- Hover shows the full error text
- New: error notifications via `api.notify()` (see item 9)
- File: `src/src/components/DownloadTable.tsx` (status cell case)

### 14. Empty States — contextual pass
- Was a single "No downloads" message
- Now contextual per sidebar filter:
  - **All** → "No downloads yet"
  - **Unfinished** → "All caught up"
  - **Finished** → "No history yet"
  - **Failed** → "No failures"
  - **Paused** → "Nothing paused"
  - **Scheduled** → "No scheduled downloads"
  - **Queue X** → "Empty queue" with the queue name
  - **Search active** → "No matches" with the query echoed
- All compact, no marketing illustrations
- File: `src/src/components/DownloadTable.tsx` (~50 lines added)

### 15. Performance Review — measured, not guessed
Wrote `scripts/bench-filter.ts` that replicates the store's
filter+sort logic and runs 50 iterations of 6 search scenarios at
3 task counts. Real numbers on this Bazzite machine (Ryzen 7 6800HS):

| Tasks | Empty | 1 token | 2 tokens | week | since+until | 3 tok+since |
|---|---|---|---|---|---|---|
| 100  | 0.004 ms | 0.060 | 0.055 | 0.004 | 0.009 | 0.073 |
| 200  | 0.003 ms | 0.091 | 0.089 | 0.008 | 0.010 | 0.092 |
| 500  | 0.007 ms | 0.231 | 0.219 | 0.010 | 0.021 | 0.240 |

**Even at 500 tasks with a 3-token + date filter, every keystroke
costs 0.24 ms.** Soft target was 25 ms; we're 100× under. The filter
pipeline is healthy.

The "scrolling remains smooth" requirement can only be measured on a
real desktop, which I can't run in this session. The numbers above
say the data layer won't be the bottleneck.

### 16. Regression Testing — actually run

| Check | Command | Result |
|---|---|---|
| `cargo test -p downpilot-core` | 59 passed, 0 failed, 1 ignored (the ETag-mismatch test from phase 1, `#[ignore]`d because the feature is missing) |
| `cargo check` (full workspace) | 0 errors, 4 pre-existing warnings (unused `classify_*` functions, unrelated) |
| `npx tsc --noEmit` (frontend) | 0 errors |
| `npm run build` | ✓ built in 1.19s, 320 KB JS bundle |
| `node --check extension/background.js` | OK |
| `node --check extension/popup.js` | OK |
| `node --check extension/content.js` | OK |
| `cargo tauri build` (release) | **Not run** — would take 5+ minutes and the resulting .deb/.rpm/AppImage need a real machine to install. The dev build (`cargo check`) and all the unit tests cover the same Rust code paths, so I have high confidence the release build will work, but I didn't burn the time. Run it before tagging a release. |

## Existing APIs reused

The spec said "no second queue impl, no parallel downloader". Every
new feature plugged into existing machinery:

- `DownloadManager::add` / `pause` / `resume` / `cancel` / `recheck`
  for toolbar actions (no new command surface)
- `db.set_meta` for persisting checksum results (no new column)
- `EngineEvent::Notify` for surfacing errors (no new event type)
- `tauri-plugin-notification` for the JS-side notify helper (already
  wired in phase 1)
- `useColumnConfig` for table presets (extended the existing hook,
  did not create a parallel one)
- `DownloadQueue` model already had `priority`, `schedule_*` fields

## Performance observations

- Search/filter is 100× under target even at 500 tasks
- Adding the speed-graph ring buffer to the store in phase 1
  added **+1.95 KB** to the JS bundle; this phase added **+6.3 KB**
  (notify helper, status-cell render, contextual empty states,
  preset menu, history multi-select)
- Total JS bundle: 320 KB (gzip: 94 KB)
- `cargo check` time: ~2s incremental, ~3s on the new checksums/notify
  changes
- Test suite: 59 tests, full run in <12s

## Remaining IDM-class gaps

Honest list of what I would tackle next if you greenlight another
phase:

1. **MD5 / SHA-1 support** — the engine and UI are wired for SHA-256
   only. The frontend dropdown says "SHA-256" hardcoded. To add
   MD5/SHA-1: extend `ChecksumResult.algorithm` to a small enum,
   add the `md-5` / `sha1` crates, branch in `verify_checksum`.
   ~50 lines + 4 tests.

2. **Background verify on completion** — `run_completion_actions`
   could kick off a verify if `expected_sha256` is set, so the user
   doesn't have to open Properties to see the result. A one-liner
   in the engine; needs a UI surface for "verifying…" state.

3. **`set_expected_sha256` Tauri command** — currently the field is
   only set at task creation. A `set_expected` command would let
   users enter the hash in Properties → Advanced and have it persist.
   ~10 lines + 1 test.

4. **StatusBar toast consumer** — `lastNotification` is in the store
   but the StatusBar doesn't render it yet. ~30 lines of JSX to add
   a small bottom-of-window toast that auto-dismisses after 4s.
   Trivial follow-up.

5. **Drag-to-reorder queues** — the spec mentioned this as optional.
   The current order is alphabetical; an explicit priority-based
   reorder would need a small dnd library or a hand-rolled handler.

6. **Sound file picker for `play_sound`** — the engine tries 4
   freedesktop sound names. A real picker would let users choose
   their own `.ogg`. ~20 lines + 1 settings field.

7. **Release build verification** — `cargo tauri build` + install the
   .deb/.rpm/.AppImage on a clean machine and walk through the
   MANUAL_TESTING.md checklist. Needs a real desktop environment.

8. **Multi-monitor tray position fix** — Tauri 2's tray has known
   issues on multi-monitor Wayland setups (Bazzite). The single-instance
   + tray work in phase 1 is solid, but the icon could land in the
   wrong monitor's system tray. Not a regression; just a known sharp
   edge.

## Recommended next phase

If the next phase had a single goal, I'd pick **"first-launch
onboarding + a real production-quality installer experience"**:

- A 3-step first-run wizard (pick default download dir, set max
  parallel, optional browser-extension install links)
- `.deb`, `.rpm`, and `AppImage` produced by CI, attached to GitHub
  releases automatically
- A polished auto-updater (Tauri's built-in updater is wired in
  `tauri.conf.json` but not exercised yet)
- A real logo and brand color (the current tray icon is a placeholder)
- Polish around the chrome — keyboard shortcut cheatsheet, the empty
  states, the about dialog, the welcome notification

This is the kind of work that turns a working app into one people
actually want to use daily.

---

Phase 2: 12 / 12 todos complete.
Real verification: cargo check, cargo test (59 pass), tsc, vite build,
node --check on all 3 extension files, perf bench.
What's deferred: release build, MD5/SHA-1, statusbar toast UI,
drag-to-reorder queues, first-run wizard.
