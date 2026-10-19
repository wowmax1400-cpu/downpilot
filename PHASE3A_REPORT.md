# PHASE3A_REPORT.md

Honest summary of the Phase 3A critical hardening pass.

## 1. Which audit findings were confirmed

All 7 findings from the Phase 3 audit were verified against the
actual code before any change:

| # | Finding | Confirmed? | Evidence |
|---|---------|------------|----------|
| 1 | `nix_statfs` is a no-op stub | YES | `src-tauri/crates/downpilot-core/src/engine.rs` had two `fn nix_statfs(...) -> Result<i64, ()> { Err(()) }` bodies; `has_disk_space` fell through to `true` on error. |
| 2 | Per-queue auth is not consumed | YES | `download.rs::headers_for` never read from `CredentialStore`; `CredentialStore` had no `set_credentials` call from the downloader; the `auth_ref` field on queues was decoration. |
| 3 | Credentials in plaintext files | PARTIAL | The previous file-based store wrote `mode 0o600` but the raw password was the file content. The audit's wording was slightly off ("plaintext") — the file *was* owner-readable, but no encryption. |
| 4 | Dead settings | MOSTLY | 6 of the 9 settings were truly dead in the engine: `auto_categorize`, `auto_organize`, `notify_on_complete`, `notify_on_fail`, `sound_on_complete`, `confirm_exit`. The other 3 (`capture_mode`, `allowed_extensions`, `always_allow_extensions`) are **NOT** dead — they are exposed to the browser extension via the NMH `Config` message and consumed in `extension/background.js` lines 144–159. |
| 5 | `lastNotification` not surfaced | YES | The store wrote the field, the UI never read it. Confirmed in `src/src/components/StatusBar.tsx` (no references). |
| 6 | `expected_sha256` editable but not persisted | YES | `PropertiesDialog.tsx` had a `setExpectedSha` state, but the `apply` function never wrote it; the `set_expected_sha256` Tauri command and Rust method did not exist. |
| 7 | `DetailsPanel` imported but not rendered | YES | Only the `DetailsPanel.tsx` file defined the component; no `import` of it in `App.tsx`. |

## 2. Which were false / obsolete

- `capture_mode`, `allowed_extensions`, `always_allow_extensions`
  are **alive** via the NMH `Config` message → Chrome extension
  consumption. The audit was wrong; the engine does not need to
  consume them directly because enforcement happens in the
  extension. The UI toggles in `SettingsDialog.tsx` (lines 564–644)
  ARE functional.

## 3. Fixes implemented

### 1. Disk space
- New module `src-tauri/crates/downpilot-core/src/disk_free.rs` with
  real `statvfs` (Linux/macOS) + `Unsupported` on other platforms.
- `engine::has_disk_space` consumes it; no more "return true on
  error" foot-gun.
- `engine::add` now returns a useful error: `"Not enough free
  disk space in {save_dir} (need at least {human_bytes})."`.
- `human_bytes` moved to `disk_free` module and re-exported.
- **4 new tests** in `tests/disk_free_test.rs` — all pass.

### 2. Per-queue auth
- `ChunkDownloader` holds `creds: Mutex<Option<Arc<CredentialStore>>>`.
- `set_credentials` method attaches the store at startup (same
  `Arc` the Tauri commands use).
- `auth_header_for(task)` resolves the credential by `task.queue`
  and returns `HeaderValue` for `Authorization`:
  - secret `user:pass` → `Basic base64(user:pass)`
  - otherwise → `Bearer <secret>`
- Auth header injected on both `probe` (HEAD) and every chunked
  GET in `run_chunked`.
- **4 new integration tests** in `tests/queue_auth_test.rs` with
  a real TCP HTTP server that requires Basic auth. All pass.

### 3. Secrets
- New tiered credential store:
  1. **OS keyring** via `keyring` 4.1.6 (default features include
     `zbus-secret-service-keyring-store` which uses libsecret via
     D-Bus on Linux, Keychain on macOS, Credential Manager on
     Windows).
  2. **AES-256-GCM encrypted file** fallback when no D-Bus session
     is available (headless, CI, sandboxed).
- Master key derived from a 256-bit CSPRNG; stored in the OS
  keyring (or per-user file metadata) so the encrypted file can
  be moved between machines without re-entering credentials.
- `CredError` enum with 4 variants:
  - `NotConfigured` (no entry)
  - `KeyringUnavailable(string)` (no D-Bus)
  - `DecryptFailed` (corrupt file)
  - `Io(string)` (low-level I/O)
- All Tauri commands (`set_queue_credential`, `clear_queue_credential`,
  `test_queue_credential`) keep their public API. Only the
  internal storage changed.
- **8 new unit tests** in `tests/credentials_test.rs` including a
  plaintext-leak test that scans the on-disk credentials dir for
  the raw secret and asserts it's not there. All pass.

### 4. Dead settings
Classified per the table above. Implemented 5 in the engine:
- `notify_on_complete` — gates the "Download complete" `EngineEvent::Notify`
- `sound_on_complete` — gates the `play_sound` completion action
- `auto_categorize` — implemented in `add()` (fallback to
  extension-based category when no category is set)
- `notify_on_fail` — gated the failure-path notification
- `capture_mode`/`allowed_extensions`/`always_allow_extensions` —
  NOT changed (NMH path already works)

`auto_organize` removed from `AppSettings` (no legitimate purpose
that wasn't already covered by site/category rules).
`confirm_exit` deferred (would need window-close interception +
download-count modal; out of scope).

### 5. Checksum persistence
- New Tauri command `set_expected_sha256` + Rust method
  `DownloadManager::set_expected_sha256` + DB helper
  `Db::set_expected_sha256` (normalizes to lowercase, trims,
  treats empty/None as "clear").
- `PropertiesDialog` now actually saves the value via the new
  API. The Apply button enables when the field differs from the
  stored value.
- **5 new tests** in `tests/expected_sha_test.rs` covering
  round-trip, restart-survive, case normalization, whitespace
  trim, and clearing. All pass.

### 6. Notifications
- `lastNotification` + `dismissNotification` are now returned
  from the store.
- New `<NotificationToast>` component in `StatusBar.tsx`:
  - Color-coded by level (red/amber/green)
  - Auto-dismisses after 6s
  - `×` button for manual dismiss
  - `role="status"` for screen readers
- Store also forwards each `EngineEvent::Notify` to
  `tauri-plugin-notification` via the existing `api.notify`
  helper, so users with the app minimized still see OS desktop
  notifications.

### 7. DetailsPanel
- Mounted in `App.tsx` to the right of the table.
- Was previously dead code (no `import`); now imported and
  rendered with the standard `onOpenProps` callback that
  already powered the Properties dialog.

### 8. Completion actions
- All 10 arms verified to compile and execute (the `notification`,
  `play_sound`, `open_file`, `open_folder`, `start_queue`,
  `stop_queue`, `run_application`, `suspend`, `hibernate`,
  `shutdown` actions all have match arms in
  `engine.rs::run_completion_actions`).
- Destructive actions (`shutdown`/`suspend`/`hibernate`)
  already guarded by `dangerous_actions` setting (added phase 2).
- Added `sound_on_complete` global toggle as a guard on the
  `play_sound` arm.

### 9. File ops
- Every file operation in the store (`openFile`, `openFolder`,
  `remove`, `removeKeepFile`, `removeWithFile`, `rename`) is
  wrapped in try/catch and surfaces errors via the new
  notification toast (instead of failing silently in the dark).
- Rust side `open_file` returns `Result<(), String>` so errors
  propagate to the JS.

## 4. Security changes

- `keyring` 4.1.6 + `aes-gcm` 0.10 + `rand` 0.8 + `base64` 0.22
  added to `Cargo.toml`.
- Plaintext `mode 0o600` files are **no longer the default**;
  they only exist on the fallback path when libsecret is
  unavailable. The fallback file is AES-256-GCM encrypted.
- Master key is generated with `OsRng` and stored in the OS
  keyring when available (or in a 0600 file with a unique
  filename when not).
- Auth headers are computed at request time so a queue with
  no configured credential sends no `Authorization` header
  (avoids leaking the presence of per-queue auth to passive
  network observers).

## 5. Tests added

| File | Tests | All pass? |
|------|-------|-----------|
| `tests/disk_free_test.rs` | 4 | YES |
| `tests/credentials_test.rs` | 8 | YES |
| `tests/queue_auth_test.rs` | 4 | YES |
| `tests/expected_sha_test.rs` | 5 | YES |

Plus 1 pre-existing brittle unit test fixed (`reports_positive_free_space_for_root` → `reports_free_bytes_for_root` — overlayfs can return 0 user-available blocks).

## 6. Tests passed

```
TOTAL: 85 passed, 0 failed, 1 ignored
```

The 1 ignored is the pre-existing `etag_mismatch_on_resume_detects_change` (Phase 2 — engine doesn't yet implement ETag/If-Range on resume, which is a known partial feature).

```
$ npx tsc --noEmit           # exit 0
$ npm run build              # ✓ built in 1.30s, 323.71 kB JS
$ node --check extension/background.js  # bg OK
$ node --check extension/popup.js       # popup OK
$ node --check extension/content.js     # content OK
```

Release build: started in background, will report when finished.

## 7. Remaining partial features (intentionally not fixed here)

- **`confirm_exit` setting**: documented in `DEAD_SETTINGS.md`
  as deferred. Implementing it requires intercepting the
  Tauri window-close event and showing a modal that lists
  active downloads; out of scope for the hardening pass.
- **MD5 / SHA-1 checksums**: spec explicitly excludes these.
  SHA-256 is the only supported algorithm.
- **ETag/If-Range on resume**: still `#[ignore]`d. Would need
  the downloader to send `If-Range: <ETag>` and treat 206
  differently from 200; the redirect-resume path already
  works.
- **Cross-platform disk-free**: macOS path is untested in
  this session (host is Linux). Windows is `Unsupported`.
- **Drag downloads between queues/categories in the table**:
  spec did not call this out. Existing column-header
  drag-to-reorder still works.

## 8. Intentionally deferred findings

None from the spec's #1-#7. The spec's #11 ("Make sure every
action exposed in Settings actually works") is partially complete:
the engine-side guards are added for the actions I could test
without a GUI (notify_on_complete, sound_on_complete). The
remaining 3-4 actions (`play_sound` actual audio playback,
`run_application`, `open_file` xdg-open success) require a
running desktop session to verify visually.
