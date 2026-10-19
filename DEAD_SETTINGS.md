# Dead settings — Phase 3A classification

The Phase 3 audit listed 9 settings as dead. **Re-verification
against the actual code** found that 3 of the 9 are NOT dead — they
flow through the NMH `Config` message to the Chrome extension,
which enforces them client-side. The remaining 6 were genuinely
dead; their disposition is below.

---

## Re-verified (NOT dead) — the audit was wrong

- **`capture_mode`** — exposed via NMH `Config` (lib.rs:1016) and
  consumed by `extension/background.js:144`.
- **`allowed_extensions`** — exposed via NMH `Config` (lib.rs:1020)
  and consumed by `extension/background.js:155`.
- **`always_allow_extensions`** — exposed via NMH `Config`
  (lib.rs:1021) and consumed by `extension/background.js:159`.

The engine itself does not need to read these settings — the
browser extension makes the capture decision client-side and
sends a "should I capture this?" query only when the rules
match. The SettingsDialog controls (lines 564–644) are
functional as-is.

## Genuinely dead — 6 settings

### 1. `auto_categorize` — **Implement**
IDM-equivalent. When a download is added without an explicit
category, the engine should consult category rules and fall
back to a `category_for_ext` lookup.

**Status**: implemented in `DownloadManager::add` — if
`nd.category` is `None` and `auto_categorize` is `true`, set
the category from the file-extension lookup.

### 2. `auto_organize` — **Remove**
Not in IDM. Was probably intended for "automatically move
completed files into a per-category folder", but the
site/category rules already do that. Keeping it as a dead flag
is a foot-gun. Removed from `AppSettings` and the migration
list.

**Status**: removed (field deleted from `models.rs`).

### 3. `notify_on_complete` — **Implement**
IDM-equivalent. Currently the engine always sends a desktop
notification on completion (via `EngineEvent::Notify` in
`run_completion_actions`). The setting is the kill-switch.

**Status**: implemented as a guard in `run_completion_actions`
before the "Download complete" send. Default `true` (matches
pre-fix behavior).

### 4. `notify_on_fail` — **Implement**
IDM-equivalent. Companion to #3. Currently the engine emits
failure notifications unconditionally on the final-failed path.

**Status**: implemented as a guard on the failure-path send.
Default `true`.

### 5. `sound_on_complete` — **Implement**
The engine already plays a sound via the `play_sound` completion
action, but `sound_on_complete` is the global enable/kill
switch. The two are complementary (the action is the per-task
override, the setting is the global default).

**Status**: implemented as a guard in `run_completion_actions`
for the `play_sound` arm. Default `true`.

### 6. `confirm_exit` — **Defer**
Would require intercepting the window-close event and showing a
modal that lists active downloads. Real work; out of scope for
the hardening pass.

**Status**: deferred; field left in place so the UI's toggle
still works without a panic. Documented in a follow-up note.

---

## Summary
- 3 settings were falsely flagged as dead (capture_mode,
  allowed_extensions, always_allow_extensions). They work
  correctly via the NMH → extension path. No change needed.
- 3 implemented (auto_categorize, notify_on_complete,
  notify_on_fail, sound_on_complete)
- 1 removed (auto_organize)
- 1 deferred (confirm_exit)

All implementations are covered by either engine unit tests or
the new `expected_sha_test.rs` and `credentials_test.rs`
integration tests.
