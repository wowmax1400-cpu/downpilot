# PHASE3_AUDIT.md — Final IDM Feature-Parity & Product Audit

> Brutally honest assessment of DownPilot against the actual target
> product: a modern Linux download manager with the capabilities and
> workflow of IDM.

**Method:** Inspected the actual repository (`~/projects/downpilot`,
2026-08-29) end-to-end: 23 frontend `.tsx`/`.ts` files (9,667 lines),
7 Rust files (6,799 lines), 11 engine tests (59 passing), the
extension (Chrome + Firefox), and the native messaging host.

**Key principle:** A feature is ✅ only if the underlying
functionality works end-to-end. A button with no working backend is
🟡. A setting that is persisted but never read is 🔴.

---

## 1. Overall maturity assessment

**DownPilot is a working application, not a prototype.** 59 engine
tests pass, `cargo check` is clean, the frontend builds, the
extension scripts parse, and most of the IDM core (queues, scheduler,
site rules, browser capture, history, completion actions, view
presets) is implemented and integrated end-to-end.

**It is also honestly incomplete in important ways.** The audit
found:

- 1 P0 bug: **disk-space pre-check is a no-op** (`nix_statfs`
  returns `Err(())` on every platform)
- 1 P0 gap: **per-queue authentication is a stub** — the
  `CredentialStore` exists, settings are exposed, but the
  `HttpDownloader` never reads them
- 1 P0 gap: **secrets are stored in plaintext files** because
  `secret-service` / `keyring` crate is not in `Cargo.toml`
- 7 settings that are persisted to the DB but **never read by the
  engine** (`capture_mode`, `allowed_extensions`,
  `always_allow_extensions`, `notify_on_complete`, `notify_on_fail`,
  `auto_categorize`, `auto_organize`, `sound_on_complete`,
  `confirm_exit`)
- 1 P1 polish gap: the `lastNotification` state added in phase 2 is
  not consumed by the StatusBar — engine-pushed notifications die in
  the store
- 1 P1 bug: the `expected_sha256` field in Properties → Advanced is
  editable in the UI but the edit does nothing (no
  `set_expected_sha256` Tauri command)

**If an experienced IDM user installed DownPilot today, they would
find that ~85% of the daily workflow works, but the remaining 15%
contains sharp edges.** They would notice:
- Adding a download works great
- Per-queue authentication doesn't work (no `Authorization` header)
- The disk space check looks like it works but doesn't
- The "Set expected SHA-256" field in Properties is a trap (looks
  editable, isn't)
- Browser capture cannot be turned "off" via the setting
- "Auto-categorize" / "Auto-organize" toggles do nothing
- Dragging a download between queues doesn't work (only columns
  can be dragged)

The application is **not ready for a public release in its current
state.** It is ready for a focused 2-3 week hardening pass to
resolve the P0/P1 items, after which it can be released as a
1.0 with confidence.

---

## 2. IDM parity matrix

See `PHASE3_IDM_PARITY.md` for the full matrix. Summary by area:

| Area | ✅ | 🟡 | 🔴 | ⚪ | Maturity |
|---|---|---|---|---|---|
| Download engine | 11 | 2 | 0 | 0 | High |
| Download management | 10 | 3 | 1 | 0 | High |
| Queues | 9 | 2 | 0 | 0 | High |
| Scheduler | 7 | 1 | 0 | 0 | High |
| Browser integration | 6 | 5 | 0 | 0 | Medium (no E2E) |
| Site management | 4 | 4 | 1 | 0 | Medium |
| Authentication | 0 | 4 | 1 | 0 | Low (stub) |
| User interface | 12 | 1 | 1 | 0 | High |
| Settings | 14 | 4 | 8 | 0 | Medium (8 dead) |

---

## 3. Top 10 remaining features (ranked by user value × IDM parity × feasibility × risk × Linux relevance)

### P0 — Critical (must fix before release)

#### 1. **Per-queue authentication: real `Authorization` header injection**
- **What:** When a queue has `auth_user` + a stored password, attach
  `Authorization: Basic base64(user:pass)` to every request for tasks
  in that queue. Also support `Authorization: Bearer ...` and per-site
  auth.
- **Why:** Many Linux file hosts (private trackers, corporate
  intranets) require login. Right now the user can enter a password
  in the UI and it's silently ignored. This is broken-by-design.
- **Files:** `download.rs:111` `headers_for`, `engine.rs:600-700`
  resolve site rule at insert, `credentials.rs`
- **Complexity:** Small (one helper, one call site in `headers_for`)
- **Dependencies:** Item #2 (secret storage) — otherwise the
  password has nowhere safe to live
- **Tests:** Mock a site rule with a credential, send a request
  through `headers_for`, assert `Authorization: Basic ...` is set

#### 2. **Real secret storage: add the `keyring` or `secret-service` crate**
- **What:** Replace the plaintext file in `CredentialStore` with
  the OS-native secret store. On Linux: `libsecret` via
  `secret-service`. On macOS: Keychain via `keyring`. On Windows:
  Credential Manager via `keyring`.
- **Why:** A download manager that stores HTTP Basic Auth passwords
  in plaintext on disk is a security incident waiting to happen.
- **Files:** `Cargo.toml` (add dep), `credentials.rs` (rewrite
  using the crate), `engine.rs:600-700` (use the new API)
- **Complexity:** Small if `keyring` is used (cross-platform);
  Medium if `secret-service` is used (Linux-only, more correct)
- **Dependencies:** None
- **Tests:** Integration test that calls `set` then `get` and gets
  back the same value

#### 3. **Disk-space pre-check: actually check the disk**
- **What:** The current `nix_statfs` returns `Err(())` on every
  platform. Replace it with a real implementation (use the
  `nix` crate's `statvfs` or call `statvfs(2)` via `libc`). On
  non-Linux, fall back to `GetDiskFreeSpaceEx` (Windows) or
  `statfs` (macOS).
- **Why:** The pre-check is currently a no-op. A user who tries to
  download a 50 GB file with 1 GB free will not get a friendly
  error — the write will fail with `ENOSPC` deep inside the
  download loop.
- **Files:** `engine.rs:749` `has_disk_space`, `engine.rs:2031-2039`
  `nix_statfs` stubs
- **Complexity:** Small (one function)
- **Dependencies:** None
- **Tests:** Unit test that calls `has_disk_space("/tmp", 1)` and
  expects true; calls with a size larger than the filesystem and
  expects false. Use a tmpfs or memory filesystem for determinism.

### P1 — High (release-quality polish)

#### 4. **StatusBar toast consumer for `lastNotification`**
- **What:** The store exposes `lastNotification` (set by every
  `EngineEvent::Notify`). Render a small bottom-of-window toast that
  auto-dismisses after 4 seconds, with different styling for
  info / warning / error levels.
- **Why:** Right now engine notifications (completion, checksum
  result, destructive-action block, file-op errors) die in the
  store. The user has no idea they happened unless they happen to
  be looking at the right dialog.
- **Files:** `StatusBar.tsx` (read `store.lastNotification`, render
  toast), `useDownloads.ts` (auto-clear after 4s)
- **Complexity:** Small (one component, ~50 lines)
- **Dependencies:** None
- **Tests:** Visual / manual (toast UI is hard to unit test)

#### 5. **Wire `set_expected_sha256` + save the editable hash in Properties**
- **What:** Add a Tauri command `set_expected_sha256(id, value)` in
  the engine and expose it via `api.setExpectedSha256`. In
  `PropertiesDialog`, the existing `expectedSha` state should be
  saved on Apply.
- **Why:** The field is editable, the UI claims it works, the
  Apply button doesn't save it. This is a "trap" UX bug — the user
  trusts the UI and gets silently-discarded edits.
- **Files:** `models.rs` (no change needed), `db.rs` (add
  `set_expected_sha256`), `engine.rs` (add
  `set_expected_sha256`), `lib.rs` (Tauri command),
  `PropertiesDialog.tsx` (call from `apply`)
- **Complexity:** Small (one DB method, one engine method, one UI
  call)
- **Dependencies:** None
- **Tests:** 1 engine test that sets a hash, re-reads the task,
  asserts the new value is persisted

#### 6. **Remove or implement the 8 dead settings**
- **What:** Decide for each:
  - `capture_mode`, `allowed_extensions`, `always_allow_extensions`
    → **implement**. The capture filter currently only honors
    `exclude_extensions` and `exclude_domains`. The "off" mode
    isn't honored either.
  - `notify_on_complete`, `notify_on_fail`, `sound_on_complete`,
    `confirm_exit` → **either implement or delete**. Right now they
    are pure noise.
  - `auto_categorize`, `auto_organize` → **implement or delete**.
- **Why:** A user who enables "Auto-categorize" and sees nothing
  change will conclude the app is broken. Settings that don't
  work are worse than no settings.
- **Files:** Varies per setting. The capture-related ones are in
  `engine.rs:2090-2120` (`grabber_filter`); the notification ones
  are in `engine.rs:run_completion_actions:1034`.
- **Complexity:** Medium (each setting has its own surface area)
- **Dependencies:** None
- **Tests:** 1 test per implemented setting

#### 7. **MD5 and SHA-1 support in verify_checksum**
- **What:** Extend `ChecksumResult.algorithm` to an enum (or string
  union). Add `md-5` and `sha1` crates. Branch in `verify_checksum`
  based on the task's `checksum_algorithm` (currently hardcoded to
  `sha256`). Update the UI dropdown.
- **Why:** Real-world download sources publish MD5 more often than
  SHA-256. apt, RPM, many CDNs. SHA-1 is still common for
  distributions.
- **Files:** `Cargo.toml` (add crates), `engine.rs:1350-1430`
  `verify_checksum`, `PropertiesDialog.tsx` (dropdown),
  `verify_checksum_test.rs` (3 new tests)
- **Complexity:** Small
- **Dependencies:** None
- **Tests:** 1 test per algorithm × 1 mismatch case = 6 new tests

#### 8. **End-to-end browser capture test on a real Chromium**
- **What:** The dev env has no Chrome. Set up a headless Chromium in
  a test harness (or a CI Docker image) and run the full capture
  flow: install extension, trigger a download, verify the
  `downpilot-nmh` relay receives the request, verify the engine
  inserts a task with the right URL and cookies.
- **Why:** Browser capture is a flagship feature. Without E2E
  testing, regressions in the NMH relay, the manifest, or the
  message format go undetected.
- **Files:** New `tests/e2e/capture.rs` (or a separate
  `extension-tests/` directory with Puppeteer or Playwright)
- **Complexity:** Medium-Large
- **Dependencies:** A test browser. Headless Chromium via
  `chromedriver` or `playwright` is the standard.
- **Tests:** 1 happy-path test, 1 capture-mode-off test, 1
  excluded-extension test, 1 cookie-forwarding test

### P2 — Useful

#### 9. **"Properties" details panel actually mounted**
- **What:** `DetailsPanel.tsx` (145 lines) is imported in `App.tsx`
  but never rendered. Either mount it in the right side of the main
  view, or delete it.
- **Why:** Dead code rots. Either ship the feature or remove it.
- **Files:** `App.tsx:13` (import), `App.tsx` body (render or remove)
- **Complexity:** Small
- **Dependencies:** None
- **Tests:** Manual

#### 10. **Drag downloads between queues/categories in the table**
- **What:** Make table rows `draggable=true`. On drop onto a
  Sidebar queue/category, call `store.moveToQueue` / `setCategory`.
- **Why:** IDM power users expect to be able to drag a finished
  download to a different category/queue without opening Properties.
  Currently you have to select → right-click → Move to Queue.
- **Files:** `DownloadTable.tsx` (row `draggable`,
  `onDragStart`), `Sidebar.tsx` (`onDrop`)
- **Complexity:** Small-Medium
- **Dependencies:** None
- **Tests:** 1 store-level test that calling `moveToQueue` updates
  the task

---

## 4. Partial features needing completion

Confirmed by inspecting the actual implementation, not assumed:

1. **File finalization has a bug.** `merge_parts:625` silently
   `continue`s past missing part files, which would produce a
   corrupt final file if a part is ever lost. Should error
   instead. (P2)
2. **`nix_statfs` is a no-op stub** on every platform. Disk
   pre-check returns `true` for every download. (P0, repeated
   for emphasis)
3. **Per-queue auth is wired in the UI but not in the downloader.**
   `CredentialStore` is constructed and managed but `HttpDownloader`
   has no `creds` reference anywhere. (P0)
4. **`expected_sha256` is editable in Properties → Advanced but
   the edit is dropped** (the `apply` function only saves `desc`
   and `saveDir`). (P1)
5. **`lastNotification` is written by the store but never
   consumed by any UI component.** Engine notifications die. (P1)
6. **8 settings are dead** (see PHASE3_IDM_PARITY.md § Settings).
7. **`DetailsPanel.tsx` is imported but never rendered.** Dead
   code, 145 lines. (P2)
8. **`eprintln!("[merge_parts] ...")` debug print in production**
   (`download.rs:626`) leaks internal paths to stderr. (P2)
9. **Two `eprintln!` in `download.rs:560, 564` are debug-only** but
   shipped in production. (P3)
10. **`capture_mode` setting is persisted but not branched on**;
    "off" mode is not honored; the engine's capture decision
    only reads `exclude_extensions` / `exclude_domains`. (P1)
11. **Per-site `save_dir` is in the model and UI but not applied
    at insert time** — same as #6, it's a dead field path. (P1)
12. **MD5 and SHA-1 are not supported in `verify_checksum`**;
    only SHA-256. The UI hardcodes "SHA-256". (P1)
13. **The PropertiesDialog editable fields are inconsistent**:
    Description, Save path, and Expected SHA-256 are all marked
    editable, but only the first two are actually saved. (P1)
14. **Recheck and Verify are confusingly named**:
    - Toolbar "Recheck" → `recheckDownload` (re-queries server size)
    - Properties "Verify SHA-256" → `verifyChecksum` (hashes file)
    Both affect the same task but do very different things. The
    names don't make this clear. (P2)
15. **No `ttauri-plugin-updater` configured** — auto-update is not
    wired. (P3, not really a feature gap, more a "production
    deploy" gap)

---

## 5. UX problems found by walking through the app

These are cases where the feature technically works but the
workflow is confusing.

1. **"Verify SHA-256" is in the wrong place.** It lives under
   Properties → Advanced. Most users will never think to look
   there. Should also be in the row context menu (right-click →
   "Verify") for the selected task.

2. **"Recheck" and "Verify" look like synonyms.** Both are right
   there in the toolbar, both modify a task's state, and the
   icons are similar (circular arrows). They do completely
   different things. Either rename one or move one out of the
   toolbar.

3. **"Edit" in the Queue row has no visual cue** that the form
   is in edit mode until you click. The "Save" / "Cancel" footer
   appears only after the click, which is a context switch.

4. **Settings dialog has 7+ sections** organized by a left-rail
   that is **not searchable in the section list** — only the
   fields below are. A user looking for "where is the proxy
   setting" has to scroll through the list. The field search at
   the top is good; the section navigation is mediocre.

5. **"Set expected SHA-256" is editable in Properties but the
   Apply button does nothing for that field.** This is a
   silent-failure trap. See #4 in §4.

6. **Right-click → "Open Folder" opens the save dir, not the
   containing folder of the file.** This is the IDM behavior but
   it surprises users on Linux where some download managers
   open the file's actual folder.

7. **Drag and drop on the table only reorders columns.** Rows
   cannot be dragged. A user trying to drag a download to a
   different queue in the sidebar will see nothing happen, with
   no feedback that the gesture wasn't supported.

8. **"Start All" in the toolbar is misleading.** It calls
   `resumeAll()` which iterates the task list and calls
   `start()` on resumable ones. But "All" implies every task —
   and completed / cancelled / failed tasks are filtered out
   silently. The label should be "Resume All" or "Start All
   Resumable".

9. **No visual indicator for queues whose schedule is currently
   active.** A schedule may be running and throttling a queue
   to 1 MB/s, but the table just shows the lower speed with
   no explanation. The StatusBar shows the active-profile
   chip, but only for bandwidth profiles, not schedule windows.

10. **The `t("prompt.renameTo")` etc. i18n strings exist but
    most of the dialogs have **hardcoded English button labels**
    ("OK", "Close", "Cancel", "Save"). This is fine for a
    single-language release but blocks translation.

11. **Light theme doesn't exist.** The CSS in `src/index.css`
    uses hardcoded dark color tokens. A user on a light
    desktop would need a separate stylesheet, which doesn't
    exist.

12. **"Re-DL" in the History row is a tiny 2-letter button.**
    A first-time user has no way to know what it means. The
    `title` attribute exists but tooltips aren't discoverable.

13. **The Settings dialog footer only has a "Close" button.**
    There is no Save / Cancel — every change is persisted
    immediately. This is the right pattern for a download
    manager (no "cancel and lose everything") but it's
    inconsistent with the Properties dialog which has Apply.

---

## 6. Workflow problems

Walking through the 6 workflows from the spec:

### Workflow A — Normal Download
- ✅ Add URL → choose location → category → queue → download →
  completion → history
- **Friction:** The "Add" dialog has fields for filename, save
  path, category, queue, threads, description, expected SHA-256.
  For a quick download the user has to scroll past 6 optional
  fields. A "Quick add" mode (Ctrl+N → just paste URL → Enter)
  would speed this up.

### Workflow B — Browser Download
- 🟡 Chrome → capture → ask/automatic → DownPilot → download →
  completion
- **Friction (assumed, not E2E tested):** The "ask" mode means
  every browser download pops a dialog. For someone downloading
  20 files from a forum, that's 20 interruptions. The
  `always_allow_extensions` setting is supposed to skip the
  dialog for known types but is not wired up (see #10 in §4).

### Workflow C — Batch
- ✅ Paste multiple URLs → preview → category/queue → download
- **Friction:** The Batch dialog has a flat list; if you paste 50
  URLs and 5 are broken, you have to find them in the list
  before clicking Download. No "validate first" button.

### Workflow D — Scheduled Download
- 🟡 Add to a queue → set a schedule → bandwidth profile → runs
- **Friction:** Schedules are configured in the **Scheduler**
  panel, not in the Queue panel. The connection between a
  schedule and its target queue is implicit (you select a
  queue in the schedule). A user expecting "queue with a
  schedule" will look in the wrong place.

### Workflow E — Failed Download
- 🟡 Failure → retry → resume → completion
- **Friction:** "Retry Failed" in the toolbar is enabled when
  *any* selected task is failed. If the user has a mixed
  selection (1 downloading + 1 failed), the button is enabled
  and clicking it retries only the failed one — but the
  button label doesn't say "Retry 1 failed (of 2 selected)".

### Workflow F — Power User
- 🟡 Multi-select → change queue → change category → bandwidth →
  pause/resume → properties
- **Friction:** Change queue / change category require
  right-click → context menu. For 10 selected tasks, that's
  fine. For 50, it's tedious. A bulk-edit dialog would help.

---

## 7. Security findings

### Current state
- **Plaintext secret storage.** `CredentialStore` writes to
  `app_data_dir/cred-<safekey>` as plaintext (because
  `secret-service` is not in `Cargo.toml`). A `cat` of the
  user data directory reveals every queue's password.
- **No proxy auth.** The proxy URL is the only thing stored;
  no support for username/password.
- **NMH on 127.0.0.1:19851.** The Tauri shell runs a loopback
  TCP server speaking the NMH frame protocol. Any local
  process can connect to 19851 and inject download URLs (or
  arbitrary commands that the Tauri shell accepts). Chrome's
  native messaging host is supposed to be stdio (one process
  per browser invocation), not a long-running server. This
  is a **local privilege escalation** if any other process on
  the machine has lower trust than the user.
- **Cookies in the task row.** The `task.cookies` field is
  plaintext in the SQLite DB. If the user copies cookies
  from their browser (a normal auth flow), the DB holds them
  in clear.
- **Logging.** 9 `eprintln!` lines in production. Most are
  fine but `eprintln!("[merge_parts] part_dir={:?}", ...)` at
  `download.rs:626` leaks internal path structure to stderr.
  Not a security issue, but operational noise.

### Hardening recommendations
- Add the `keyring` crate for cross-platform OS secret
  storage. **Do this first.**
- Bind the NMH server to a **Unix socket** in a per-user
  directory with `0600` permissions, instead of a TCP port
  on localhost. Chrome's NMH spec doesn't directly support
  Unix sockets, so this requires a small NMH shim process
  that bridges stdio ↔ socket.
- Encrypt the SQLite DB with SQLCipher. The credentials.rs
  pattern is fine for secrets but cookies in the task row
  are a different concern.
- Replace `eprintln!` with a proper `log` crate façade so
  the user can control verbosity.

---

## 8. Technical debt (selected, in priority order)

| Item | Severity | File |
|---|---|---|
| `nix_statfs` no-op stub | P0 | `engine.rs:2031` |
| `CredentialStore` not consumed by downloader | P0 | `download.rs` (no `creds` references) |
| `secret-service` / `keyring` not in `Cargo.toml` | P0 | `Cargo.toml` |
| `lastNotification` written but not rendered | P1 | `useDownloads.ts` ↔ `StatusBar.tsx` |
| `set_expected_sha256` missing | P1 | (no file) |
| 8 dead settings (defined, persisted, never read) | P0–P2 | `models.rs:440-445` |
| `DetailsPanel` imported but never mounted | P2 | `App.tsx:13` |
| `eprintln!` debug line in `merge_parts` | P2 | `download.rs:626` |
| `eprintln!` in `download.rs:560, 564` | P3 | `download.rs` |
| `eprintln!` in `lib.rs:538, 698, 708, 1058, 1069, 1081, 1303` | P3 | `lib.rs` |
| `recheck` (toolbar) vs `verify` (Properties) naming collision | P2 | `Toolbar.tsx` / `PropertiesDialog.tsx` |
| "Start All" label is misleading (it skips completed/cancelled) | P2 | `Toolbar.tsx` |
| Inconsistent "Save" semantics: Settings auto-saves, Properties has Apply | P3 | `SettingsDialog.tsx` / `PropertiesDialog.tsx` |
| Long `lib.rs` (1420 lines) | P3 | `lib.rs` could be split into commands.rs / events.rs / nmh.rs |
| `mix_completion_actions` has 10 arms in one function | P3 | `engine.rs:1034` could be a match table |

---

## 9. Performance findings

Already measured in phase 2 (see `scripts/bench-filter.ts`):

- 500 tasks, 3-token search + 2 date filters + sort: **0.24 ms/pass**
- 100 tasks, single text token: **0.06 ms/pass**
- All scenarios are 100x under the soft target.

**Other potential hot spots not yet measured:**

1. **`store.setTasks([...tasksRef.current])` on every progress
   event.** Spread-clones the array. With 200 active tasks at
   150ms throttle = 1,333 array clones/sec. Probably fine but
   worth measuring.
2. **`Sidebar.tsx` re-renders on every `tasks` change** because
   it reads `store.counts`. If `counts` deps are wrong, every
   render recomputes. Quick `useMemo` review needed.
3. **Speed graph ring buffer (phase 1) bumps `setSpeedHistoryVersion`
   on every throttled (2 Hz) progress event.** That triggers a
   full table re-render. With 50 active tasks = 100 re-renders/sec
   for the table. The graph SVG itself is cheap (<0.5ms each per
   my bench data) so this is fine in practice, but a
   `React.memo` on the row could cut the constant.

None of these are user-visible today. The benchmark data is the
real evidence: the filter pipeline is healthy.

---

## 10. Recommended development roadmap

### Phase 3A — Critical Gaps (2 weeks, 1 person)
- **P0-1**: Wire `CredentialStore` → `headers_for` for real
  per-queue `Authorization` injection
- **P0-2**: Add `keyring` crate, replace plaintext secret storage
- **P0-3**: Fix `nix_statfs` (real `statvfs` via `libc` or `nix`
  crate)
- **P0-4**: Remove or implement 8 dead settings (keep
  `capture_mode`, `allowed_extensions`, `always_allow_extensions`;
  delete the rest)
- **P1-1**: `set_expected_sha256` Tauri command + Properties
  Apply wiring
- **P1-2**: StatusBar toast consumer for `lastNotification`
- **P1-3**: Capture decision honors `capture_mode` (off/ask/auto)

**End state:** 1.0 release candidate. All P0/P1 gaps closed.

### Phase 3B — IDM Power Features (2 weeks)
- MD5 + SHA-1 in `verify_checksum`
- Drag downloads between queues/categories in the table
- Bulk-edit dialog for multi-selected tasks (queue/category/threads)
- "Quick Add" mode for `Ctrl+N` (skip optional fields)
- Light theme
- i18n: make all button labels translatable
- `ttauri-plugin-updater` for auto-update
- E2E browser capture test on headless Chromium (CI)

### Phase 3C — UX Polish (1 week)
- Mount or delete `DetailsPanel`
- Rename "Recheck" / "Verify" / "Re-DL" for clarity
- Better "Start All" label
- Validation in the Batch dialog
- Schedule queue ↔ schedule panel UX alignment
- Accessibility pass: focus traps in dialogs, keyboard navigation
  in the table, `aria-live` for toasts

### Phase 3D — Final Hardening (1 week)
- NMH over Unix socket with 0600 perms (replaces 127.0.0.1:19851)
- SQLCipher for the DB
- Replace `eprintln!` with `log` crate
- Real secret rotation policy (when `keyring` is in)
- Linux package metadata polish (`.desktop` file icon,
  AppStream data, MIME types)
- First-run wizard (download dir, max parallel, optional
  browser extension install)
- Real logo + brand color

---

## 11. The ultimate question

> If an experienced IDM user installed DownPilot today, what would
> they still miss?

### The short answer
A working password manager for HTTP Basic / Bearer auth (P0-1,
P0-2), a real disk space check (P0-3), a `set_expected_sha256`
command (P1-1), a status bar toast (P1-2), a way to disable
browser capture (P1-3), and MD5/SHA-1 verification (Phase 3B).

### The honest long answer
**About 85% of the IDM daily workflow works.** The remaining 15%
is concentrated in:

1. **Per-queue authentication** (broken by design — the UI accepts
   the password but the request never carries it)
2. **Eight dead settings** that the user might toggle and then
   conclude the app is broken
3. **One trap-UI bug** (editable hash field that doesn't save)
4. **One dead UI component** (DetailsPanel — either mount it or
   delete it)
5. **The "off" mode of browser capture** is a no-op (the engine
   doesn't honor it)
6. **MD5 / SHA-1 verification** (most public download sources
   publish MD5)
7. **Drag downloads between queues** (only columns can be
   dragged, not rows)
8. **Auto-update** (not wired)

None of these is a 3-month project. The P0 list (items 1, 2, 3)
is honestly 2-3 days of focused work. The P1 list is another
week. The P2/P3 list is polish.

**DownPilot is a serious, well-architected application with a
small number of sharp edges.** The 1.0 release is achievable in
2-3 weeks of focused hardening.

---

Phase 3: 11 / 13 deliverables complete.

What's complete: full IDM parity matrix, top-10 ranked, partial
features identified, UX problems, workflow problems, security
findings, tech debt, performance findings, roadmap.
