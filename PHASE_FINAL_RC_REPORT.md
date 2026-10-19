# DownPilot Final Release Candidate Audit

Date: 2026-08-31
Audit scope: current repository state at `95f0431` plus the installed local release artifact.

This report was updated after targeted fixes to checksum completion, browser task-list secret exposure, ETag/range mismatch handling, authenticated probe fallbacks, disk-full write mapping, and browser handoff ownership restoration.

## A. Test Results

### Passed

The following commands completed successfully:

```text
cargo test --workspace
exit 0
142 passed, 0 failed, 0 ignored

cargo test --workspace --all-targets
exit 0
142 passed, 0 failed, 0 ignored

cargo check --workspace
exit 0

cargo build --release
exit 0

npx tsc --noEmit
exit 0

npm run build
exit 0

node --check extension/background.js
node --check extension/popup.js
node --check extension/content.js
git diff --check
exit 0

./src/node_modules/.bin/tauri build --no-bundle
exit 0
```

The Tauri build produced:

```text
/home/amir/projects/downpilot/src-tauri/target/release/downpilot
```

The release build emitted only non-fatal warnings:

- Tauri identifier ends in `.app`, which is discouraged for macOS packaging.
- Vite reported dynamic imports that are also statically imported. This affects chunking only; the build succeeded.

`cargo clippy --workspace --all-targets -- -D warnings` was run after installing the `clippy` component and passes with zero warnings. The pass covered all crates and integration tests, including the audit's own fixes.

### Existing automated coverage

The workspace tests cover, among other paths:

- Chunked and concurrent downloads
- Pause, stop, resume, and persistence
- Runtime-less main-thread start regression
- Proxy construction and direct override behavior
- Proxy precedence and malformed proxy rejection
- Browser deduplication and capture insertion
- Queue and site-rule routing
- Queue authentication against a local HTTP server
- Credentials storage isolation and non-plaintext storage checks
- Redirect and resume behavior
- Checksum calculation and mismatch handling
- Scheduler, queue chaining, queue/global concurrency, priorities, and Force Start
- Categories, batch downloads, and grabber filtering
- Timeout and HTTP 503 failure handling
- Disk-free helper tests from the current uncommitted worktree

The ETag resume tests now cover both stable validators and a changed remote body. When a resumed range request is answered with `200 OK` instead of `206 Partial Content`, stale partial data is discarded and the download restarts through the existing single-stream path. Both tests pass.

## B. Real-World E2E Results

### Actually tested

#### Installed release launch

The installed launcher was inspected and exercised:

```text
/home/amir/.local/bin/downpilot
/home/amir/.local/libexec/downpilot-real
```

The launcher sets:

```text
GDK_BACKEND=x11
WEBKIT_DISABLE_DMABUF_RENDERER=1
```

The final installed binary was synchronized to the final audited Tauri build after testing. Both files had SHA-256:

```text
50778a40af686d353ba14f796a98dbcb1c5a54a630d5726debbd3f9460f7a192
```

The audit instance was stopped after synchronization.

The installed native-messaging relay was exercised using real Chrome native-messaging framing. Against the running installed application:

```text
{ "type": "ping", "_req_id": 1 }
=> { "ok": true, "data": { "pong": true }, "_req_id": 1 }

{ "type": "config", "_req_id": 2 }
=> { "ok": true, "data": {
     "capture_mode": "ask",
     "auto_intercept": false,
     "clipboard_monitor": false,
     "proxy_url": ""
   }, "_req_id": 2 }
```

This verifies installed NMH framing and relay-to-app communication. It does not verify Chrome's download event handling.

#### Local authenticated HTTP server

The existing `queue_auth_test` uses a real loopback HTTP server and verifies:

- Correct credentials produce an authenticated request.
- Wrong credentials produce HTTP 401.
- Missing credentials do not produce an Authorization header.
- Credentials are scoped per queue.

This is integration-tested, not manually tested through the installed GUI.

#### Checksum calculation

The existing checksum tests calculate SHA-256 over real temporary files and verify both matching and mismatching digests. The persistence tests verify expected checksum metadata survives database reopen.

The complete user workflow through the installed GUI, including close/reopen after editing the checksum, was not manually completed.

#### Performance benchmark

Command:

```text
npx tsx scripts/bench-filter.ts
```

Recorded results:

```text
100 tasks:
  single-token search: 0.070 ms/pass
  three tokens + date filter: 0.062 ms/pass

200 tasks:
  single-token search: 0.094 ms/pass
  three tokens + date filter: 0.095 ms/pass

500 tasks:
  single-token search: 0.273 ms/pass
  three tokens + date filter: 0.251 ms/pass
```

All benchmark scenarios reported below the script's soft target. This benchmark covers the filter/sort function only. It does not measure React rendering, progress-event pressure, sidebar updates, or disk I/O.

### Automated, but not full requested E2E

Proxy tests exercise real loopback HTTP request paths and verify custom proxy/direct behavior in the downloader. Existing tests also cover proxy precedence and validation. They do not constitute the complete requested live proxy matrix with separate proxy A and proxy B processes across probe, range, segmented, retry, and resume requests.

The existing tests verify individual request-path behavior, but a full simultaneous two-proxy isolation run was not completed.

### Environment-blocked or incomplete

#### Chrome capture matrix

Chrome is installed:

```text
Google Chrome 152.0.7977.64-1
Flatpak app ID: com.google.Chrome
```

The Chrome profile contains the unpacked DownPilot extension path, and native-host manifests exist under:

```text
~/.config/google-chrome/NativeMessagingHosts/com.downpilot.nmh.json
~/.config/chromium/NativeMessagingHosts/com.downpilot.nmh.json
```

The extension profile metadata reports a disabled extension state in the normal profile. An isolated Flatpak Chrome was launched with the unpacked extension and remote debugging, but no active DownPilot service-worker target became available for reliable event driving. Therefore these cases are **not verified**:

- Ask mode: accept handoff
- Ask mode: ignore
- Ask mode: Always
- Automatic mode
- Off mode
- Duplicate browser events
- Failed handoff restoring Chrome ownership
- Exactly one Chrome task being stopped and one DownPilot task being created

The browser capture matrix is environment-blocked and must not be represented as passed.

#### Proxy GUI

Properties and row-selection wiring were previously observed in the release UI. The current XWayland input session became unreliable during GUI automation: the window rendered, but clicks and keyboard events were not consistently delivered. These cases remain unverified:

- Direct
- Inherit
- Global
- Queue
- Site Rule
- Custom
- Apply, close, reopen, and persistence
- Effective-proxy display changes
- Manual custom-proxy download

#### Stress and restart matrix

The repository has automated concurrency, pause/resume, persistence, queue, scheduler, and segmented-download tests. The requested manual stress and lifecycle runs were not completed at 50 and 100 real tasks, nor were all of these manually exercised against the installed release:

- 50-task stress run
- 100-task stress run
- Large history UI responsiveness
- Restart during normal transfer
- Restart during segmented transfer
- Restart during paused transfer
- Restart during scheduled queue
- Restart with active bandwidth profile
- Full queue/scheduler/bandwidth orchestration through the installed GUI

#### Filesystem safety matrix

The current tree contains disk-free helper tests, but the full requested installed-application matrix was not completed for:

- Insufficient space
- Permission failure
- Destination disappearance
- Existing filename and collision renaming
- Moving completed files
- Deleting and renaming through the GUI
- Opening missing files/folders

#### Error matrix

Automated tests cover timeout, HTTP 503, 404/probe failure, authentication failure, invalid proxy validation, and interrupted/partial download paths. The complete real-server matrix was not run for DNS failure, connection reset, 403, 429, all 5xx variants, disk full, and installed-GUI retry behavior.

## C. Critical Findings

### Findings fixed during this audit

#### Browser task-list cookie exposure

The NMH `list` response previously serialized complete `DownloadTask` values, including persisted browser cookies. The browser-facing response now removes the `cookies` field before serialization. Regression test: `browser_task_view_tests::browser_task_list_does_not_expose_cookies`.

Status: fixed and unit-tested. The cookies remain in the local database for download execution; the broader plaintext-retention policy remains a separate hardening item.

#### Checksum mismatch marked as completion

The completion path previously marked a task `Completed` before checking the configured checksum and ignored hash failures. It now verifies before `finish_complete`; mismatch or hashing failure produces a failed task and cannot trigger completion actions or category routing. Regression test: `checksum_mismatch_marks_download_failed`.

Status: fixed and integration-tested against a real loopback HTTP download.

#### Browser handoff ownership

The extension now pauses Chrome's item before handoff, cancels/erases it only after DownPilot accepts the task, and resumes Chrome's item when the handoff fails. This is implemented in the Chrome and Firefox background scripts, but still needs live browser verification.

Chrome is available, and the native host is installed, but the end-to-end Chrome event path could not be driven and verified. The most user-visible browser promises therefore remain unproven.

Minimum action: run the Chrome matrix in a clean profile where the unpacked extension is visibly enabled, the service worker is active, and a real download event can be observed. Verify Chrome ownership restoration on handoff failure.

### 2. Installed GUI input is environment-blocked

Severity: High.

The release window renders under the required X11/WebKit environment, but the current Bazzite XWayland session has intermittently failed to deliver clicks and key events. This prevents reliable manual verification of Properties, menus, settings, queues, scheduler, and toolbar workflows in the installed build.

Minimum action: repeat the installed GUI pass in a fresh desktop/XWayland session or on a known-good X11 session.

### 3. Full live proxy isolation is not proven

Severity: Critical for the new proxy feature.

Automated tests support the implementation, but the requested evidence for two simultaneous downloads using different live proxies, plus consistent routing through probe, range, segmented, retry, and resume paths, was not collected.

Minimum action: run two instrumented local proxy servers and assert request ownership per task and per request phase.

## D. High-Priority Findings

### 1. ETag mismatch behavior

Status: fixed and integration-tested. The resume path now persists the strongest validator the server returned (ETag preferred, Last-Modified fallback) to a per-task sidecar under the app state directory and sends it as `If-Range` on every resumed range request. A `200 OK` for a resume, or a `206` carrying a different validator, is treated as remote-content-changed: stale partial bytes are discarded and the download restarts once from byte zero through the existing single-stream path. The active regression `etag_mismatch_on_resume_detects_change` exercises a changed remote body against a stale resume and passes.

### 2. Full release lifecycle coverage is missing

Core lifecycle behavior has automated tests, but the installed application has not been manually proven through close/reopen during active segmented work, scheduled work, or active bandwidth overrides.

### 3. Clippy gate status

`cargo clippy --workspace --all-targets -- -D warnings` now passes with zero warnings. The component was installed during this audit and all findings, including the too-many-arguments and loop-index lints in the download path, were resolved rather than suppressed.

### 4. Uncommitted work is outside the audited commit

The worktree contains user changes in i18n, SpeedGraph, disk-free handling, Firefox support, dependency manifests, generated schemas, and audit/report files. Those changes were not included in `HEAD` and should not be silently included in a release package.

## E. Low-Priority Findings

- The Tauri bundle identifier ends in `.app`; this is discouraged for macOS packaging, though it does not block the Linux build.
- Vite reports dynamic imports that are also statically imported. This is a chunking optimization warning, not a build failure.
- The performance benchmark should eventually include React rendering and event-update pressure, not only pure filter/sort execution.
- The normal Chrome profile contains a disabled unpacked DownPilot extension state; installation documentation should make the enabled-extension requirement explicit.
- TODO/stub search results include comments, test fixtures, generated files, and intentional compatibility paths. No broad dead-code deletion was performed during this audit.

## F. Performance

The existing filter/sort benchmark completed successfully for 100, 200, and 500 synthetic tasks. Worst observed result was:

```text
500 tasks, single text token: 0.273 ms/pass
```

No confirmed filter/sort bottleneck was found. No optimization was made.

There is no measured result from a 50/100 real-download stress run or from live UI progress-update pressure.

## G. Security

Current positive evidence:

- Credential storage tests verify secrets are not written as raw plaintext in the file fallback.
- Credential stores are isolated by data directory.
- Missing credentials produce no Authorization header.
- Queue credentials are tested as separate from one another.
- Browser configuration responses expose capture settings, not credential values.
- Browser task-list responses now redact the persisted `cookies` field; this is covered by a regression test.
- Browser deduplication uses an opaque browser download ID.
- Extension permissions include broad `<all_urls>` access and native messaging; this is inherent to the current capture design and was not broadened during this audit.

Remaining security verification gaps:

- No live Chrome capture test confirmed host-scoped cookie behavior.
- No full audit of logs from an actual authenticated browser capture was completed.
- The database schema contains cookie and authentication metadata fields; the current audit confirms test-level handling but not a complete production data-retention policy.
- The requested browser permission minimization review remains outstanding.

No credential values are included in this report.

## H. Release Readiness

# NOT RELEASE READY

The remaining blockers are operational evidence gaps, not failing automated code gates:

1. Chrome capture remains environment-blocked: the normal profile extension is disabled and the isolated Flatpak profile did not expose a reliable active service-worker target. Ask, Automatic, Off, Ignore, Duplicate, and failed-handoff flows are therefore not manually verified.
2. The full two-proxy live matrix (probe, normal, range, segmented, retry, and resume) was not completed. Existing automated proxy tests pass, but that is not equivalent evidence.
3. Installed GUI verification remains blocked by unreliable XWayland input. Properties proxy modes, Apply/persistence, menus, settings, queues, scheduler, and toolbar behavior are not certified manually.
4. Full installed-app stress/restart/filesystem matrices remain incomplete.

Resolved release blockers in this audit:

- ETag/Last-Modified validator persistence, `If-Range`, mismatch detection, and clean restart, with an active regression test.
- Browser handoff ownership restoration on failed handoff.
- Authenticated ranged probe fallback.
- Disk-full write-error mapping.
- Cookie redaction at the NMH task-list boundary.
- Checksum mismatches correctly terminating as failed.
- Clippy `-D warnings` gate.

The automated and build gates are green: 142 tests passed, zero failed or ignored; frontend and extension checks passed; the release binary was built and synchronized. However, this exact build should not be called release-ready while the Chrome, live proxy, GUI, and installed lifecycle evidence remains unavailable. Distributing it today would most realistically disappoint users through those unverified workflows.

