# DownPilot Phase 3B Audit — Queue & Scheduler Surface

Goal of Phase 3B: turn the existing queue + scheduler + bandwidth-profile
subsystems into a coherent IDM-class orchestration system. Audit before
changing anything — what already works, what is incomplete, and what is
missing must be enumerated before code is written.

This audit was produced against the state of the tree as of the
start of Phase 3B, after fixing the broken build (the `priority`
column had been added to the `DownloadTask` model + a new migration,
but the migrations array length, the `default_for_probe` struct
initialiser, and the row-mapper in `db.rs` were left out of sync, so
`cargo build` failed out of the gate — fixed as part of step 0 below).

## 0. Build hygiene

| Item                                  | State                          |
|---------------------------------------|--------------------------------|
| `cargo check`                         | clean after fixing migrations  |
| `cargo build`                         | clean after fixing migrations  |
| `cargo test --workspace`              | 91 pass, 1 ignored, 0 fail     |
| `npx tsc --noEmit`                    | clean                          |
| `node --check extension/background.js`| clean                          |
| `node --check extension/popup.js`     | clean                          |
| `cargo build --release`               | builds                         |

The 1 ignored test is `submit_browser_download_without_skip_probe_fails_on_404`
in `tests/chunked_test.rs` (a network-probe test that is too slow / non
deterministic to keep in the default test set — kept ignored for
historical reasons, not affected by Phase 3B).

The async integration tests (`chunked_test`, `etag_resume_test`,
`redirect_resume_test`, `browser_capture_test`, `credentials_test`,
`expected_sha_test`) need a larger thread stack than the default
8 MiB tokio test thread provides; the workspace `.cargo/config.toml`
now exports `RUST_MIN_STACK=8388608` for `cargo test` invocations.

## 1. Inventory — what already works

### Queue CRUD + persistence
- `DownloadQueue` model: `name`, `label`, `max_simultaneous`,
  `speed_limit_kbps`, `enabled`, `priority`, `schedule_enabled`,
  `schedule_start`, `schedule_end`, `on_complete`, `on_complete_command`,
  per-queue auth (proxy / cookies / user_agent / auth_ref / auth_user /
  `has_password` flag).
- SQLite migrations add `queue` to downloads, seed the `general` queue
  at index 1, and add per-queue auth metadata columns.
- DB methods: `list_queues`, `get_queue`, `upsert_queue`,
  `delete_queue`, `set_queue_enabled`, `set_queue_speed`,
  `move_to_queue(id, queue)`.
- Manager API: `list_queues`, `create_queue`, `update_queue`,
  `delete_queue`, `set_queue_enabled`, `set_queue_speed`,
  `resume_queue`, `pause_queue`, `stop_queue`.
- Tauri commands: `create_queue`, `update_queue`, `delete_queue`,
  `set_queue_enabled`, `resume_queue`, `pause_queue`, `stop_queue`,
  `set_queue_speed`, `effective_queue_speed`.
- Frontend: `QueuesPanel` with full editor (max simultaneous, bandwidth
  cap, priority numeric, enabled toggle, schedule start/end, per-queue
  auth section with keyring-backed password storage), live counts
  (active/queued/total), per-row Start/Pause/Stop.

### Concurrency governor (`govern_concurrency` every 400 ms)
- Global cap: `settings.max_simultaneous` (0 = unlimited).
- Per-queue cap: `queue.max_simultaneous`.
- `start()` itself re-checks both caps before flipping a task to
  Downloading (defence in depth against the 400 ms tick missing the
  cap).
- Disabled queues park tasks as Queued and don't promote them.

### Per-queue bandwidth cap
- `downloader.set_queue_speed(queue, kbps*1024)` is the single sink.
- `effective_queue_speed_kbps(queue)` returns either the active
  scheduled override or the configured baseline — exposed to the UI as
  `effective_queue_speed`.

### Bandwidth profiles / schedules
- `Schedule` model: `id`, `name`, `queue` (legacy single-queue target),
  `enabled`, `kind` (daily/weekly/once), `start_time`, `end_time`,
  `days` (Sun..Sat bitmask, 127 = every day), `action`
  (start/stop/set_bw/bandwidth), `bandwidth_kbps` (legacy), and the
  modern `overrides_json` (queue → kbps map), `priority`, `on_complete`,
  `on_complete_command`.
- DB: `list_schedules`, `upsert_schedule`, `delete_schedule`.
- `enforce_bandwidth_profiles` runs every pump tick.
- Overlap precedence: **higher `priority` wins; ties resolve by older
  `id` first** (deterministic, documented in code comment).
- Baselines captured on first activation; restored on deactivation
  without overwriting user edits made during the active window.
- `active_profiles()` returns the schedules + affected queues that
  are currently active — the UI shows a "Night Profile active" badge
  driven entirely by this snapshot.

### Per-queue schedule (legacy single-queue action)
- `enforce_schedules` runs every pump tick.
- Per-queue `schedule_enabled` + `schedule_start` / `schedule_end`
  allow a queue to run only inside its own window.
- Inside the window: `should_run = q.enabled`.
- Outside the window: auto-pause active downloads via `auto_paused`
  set; auto-resume when the window re-opens.

### Schedule action: bandwidth override per queue
- `action="set_bw"` (or modern `overrides_json` entries) call
  `set_queue_speed` directly inside the pump tick. Idempotent, so
  safe to re-apply every 400 ms.

### Time-window math
- `time_in_window(start, end)` handles same-day windows
  (`s <= e`) and overnight windows (`s > e`, e.g. 23:00–06:00).
- Weekly schedules honour the day-of-week bitmask.
- One-time schedules flip on `s.start_time == "due"` /
  `s.start_time == now()`.

### Completion actions
- Per-download `on_complete` (JSON: action + command + target_queue)
  with actions: none / open_folder / play_sound / run_script /
  start_target_queue / pause_target_queue / notify.
- Per-queue `on_complete` + `on_complete_command` with the same set
  (none / open_folder / play_sound / run).
- Triggers from `finish_task` only on `Status::Completed`. Sound /
  notification honours `settings.sound_on_complete` /
  `notify_on_complete` (both default true).

### Live scheduler status
- `manager.active_profiles()` returns `Vec<ActiveProfile>` with
  `schedule_id`, `name`, `priority`, `affected_queues`. UI shows
  per-schedule "Active" badge + per-queue override in
  `QueuesPanel`. The pump loop already calls this every 400 ms.

### Global schedule (settings-level)
- `settings.schedule_enabled` + `settings.schedule_start` /
  `settings.schedule_end` (legacy) drive a single global pause/resume
  window on top of `enforce_schedules`.

## 2. Inventory — incomplete or missing

These are the gaps Phase 3B has to close without rewriting anything:

1. **No real Force Start.** The store-side `forceStart(id)` is a
   comment-only stub (`api.resumeDownload(id).catch(() => api.redownload(id))`)
   that doesn't bypass the concurrency gate.
2. **No download-level priority inside queues.** The `priority` column
   on `downloads` was added to the schema and the model, but the
   SELECT statements + the row mapper + `default_for_probe` were left
   out of sync (this audit fixed those), and `govern_concurrency`
   doesn't actually read it.
3. **Queue ordering is one knob (`priority`) but no UI for
   reordering** (move up / down / top / bottom). The DB stores
   `priority` (used as a sort key by `list_queues`), but there is no
   API to manipulate it.
4. **No queue chaining.** There is no "start queue B when queue A
   finishes" mechanism.
5. **Schedule action types are limited to `start` / `stop` /
   `set_bw`.** The spec wants per-queue schedule to support pause /
   resume, not just start/stop. (Pause is already implicit because
   anything not "should_run" gets paused; a separate `pause` action
   is mostly for clarity.)
6. **No live "currently scheduled" badge for queues** — only for
   bandwidth profiles. The per-queue schedule status is encoded in
   `auto_paused` and the queue's `enabled` flag but not surfaced as
   a single string to the UI.
7. **Queue editor doesn't show:**
   - the queue's currently effective bandwidth (configured vs.
     scheduled vs. measured),
   - the queue's schedule window + active/inactive state,
   - the queue's auth status (other than the password dot),
   - the queue's per-queue priority (it has the field but no visual
     cue).
8. **No deterministic concurrency / priority / scheduling tests.**
   The 91 existing tests cover download mechanics (chunked, retry,
   resume, capture, credentials, bandwidth profile persistence) but
   do not exercise the governor directly.
9. **`active_profiles()` only returns bandwidth-profile schedules.**
   It does not return queue-schedule status (active / waiting /
   outside-window) — the UI cannot tell from the same source that a
   queue is currently held by its own schedule.

## 3. Implementation plan (do not duplicate what works)

1. **Fix the build** (done — migrations count + struct initialisers +
   SELECT statements + row mapper).
2. **Download priority:**
   - Sort queued tasks in `govern_concurrency` as
     `(queue.priority DESC, task.priority DESC, created_at ASC)`.
   - `NewDownload.priority: Option<i32>` + `Db::set_priority(id, p)`.
   - Tauri command `set_download_priority` for the UI.
3. **Force Start:**
   - `manager.force_start(id)` — flips the task to Downloading and
     spawns `run_task`, *only* if the safety limits admit it. The
     trick: bypass the *queue* limit if and only if the *global*
     limit still has headroom. We deliberately keep the global cap
     so the user can't accidentally fork-bomb the box.
   - Tauri command `force_start_download(id)` + store wrapper
     `forceStart(id)`.
   - State flag `force_started: bool` on `DownloadTask` so the UI
     can render a "Force Started" badge without changing priority.
4. **Queue ordering:**
   - `manager.reorder_queue(name, target_index)` — renumbers the
     `priority` field by list index (lower index → lower priority
     number → listed first). Persisted via `upsert_queue`.
   - Tauri commands `move_queue_up/down/top/bottom`. Frontend
     gets up/down buttons in `QueuesPanel` and drag-reorder via
     native HTML5 drag-and-drop on the queue grid.
5. **Queue chaining:**
   - `DownloadQueue.next_queue: Option<String>` — when the queue
     empties, run `resume_queue(next)`. Persisted in SQLite.
   - `engine.maybe_queue_completed(queue)` already fires when a
     task completes / fails / cancels; extend it to invoke the
     chained-queue start after the last task leaves. Reuse
     `run_completion_actions` (don't fork).
6. **Schedule action types:**
   - Add `pause` / `resume` to `Schedule.action`. The existing
     "should_run" branch already implements both via
     `auto_paused`; the new actions are *exposed* so the UI lets
     the user write "23:00 → pause, 06:00 → resume" without
     inventing a fourth state. Same internal handling as `stop`
     and `start` respectively.
7. **Live scheduler status for queues:**
   - `manager.queue_schedule_status(name)` returns a struct with
     `enabled`, `schedule_enabled`, `in_window`, `paused_by_schedule`,
     `next_change_at_min` (minutes since midnight).
   - Tauri command `queue_schedule_status(name)` returns the same.
   - Use it for the "Scheduled / Waiting / Active" badge in the
     queue row + the queue editor.
8. **Bandwidth display integration:**
   - The pump loop already writes the active override map; expose
     it as `manager.queue_speed_snapshot(name)` returning
     `{ configured_kbps, scheduled_kbps, current_kbps }`.
   - The UI in `QueuesPanel` already calls `effective_queue_speed`
     but doesn't show the breakdown; switch it to the snapshot.
9. **Queue details panel:**
   - Add a compact status block per queue row: active/queued/max,
     configured bandwidth + scheduled bandwidth (if any) + measured
     bandwidth, current schedule window, auth state.
10. **Tests:** `tests/queue_orchestration_test.rs` — deterministic,
    covers everything listed in the Phase 3B §20 list without
    depending on real HTTP. Use a local tokio TCP echo / never-finish
    server where the test needs a download to be live.
11. **Regression:** do not touch the download engine, completion
    actions, persistence layer, or the browser-capture path.

## 4. Out of scope

- New auth schemes.
- Cloud / remote queue sync.
- Per-host/per-file priority finer than the three buckets.
- Scheduler "conflict wizard" beyond deterministic precedence.
- Anything that would require a schema migration beyond the
  `priority` column that already exists.

## 5. Verification commands

```
cargo test --workspace
cargo check
npx tsc --noEmit
node --check extension/background.js
node --check extension/popup.js
cargo build --release
```

All must be clean. The async test stack is bumped in
`src-tauri/.cargo/config.toml`.