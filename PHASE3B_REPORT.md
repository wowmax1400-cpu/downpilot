# DownPilot — Phase 3B Report: Advanced Queue Orchestration

## Verification (run on 2026-08-30)

- `cargo test --workspace` — **107 passed, 0 failed, 1 ignored**
  (16 of those are the new Phase 3B integration tests in
  `crates/downpilot-core/tests/queue_orchestration_test.rs`).
- `cargo check` — clean (4 pre-existing dead-code warnings, unrelated).
- `cargo build --release` — clean, release binary in `target/release/downpilot`.
- `npx tsc --noEmit` (in `src/`) — clean.
- `node --check extension/background.js` — clean.
- `node --check extension/popup.js` — clean.

## 1. What already worked (before Phase 3B)

The audit in `PHASE3B_AUDIT.md` mapped every existing piece of the queue
and scheduler infrastructure. Confirmed-working at audit time:

| Feature | Where | State |
|---|---|---|
| Queue CRUD (`create_queue` / `update_queue` / `delete_queue`) | `engine.rs:1723-1750` | working |
| Queue ordering via `priority` integer | `db.rs:list_queues` `ORDER BY priority ASC` | working |
| Per-queue `max_simultaneous` | `govern_concurrency` cap walk | working |
| Global `max_simultaneous` (`AppSettings`) | `govern_concurrency` cap walk | working |
| Per-queue bandwidth cap (`speed_limit_kbps`) | `DownloadManager::set_queue_speed` | working |
| Bandwidth profiles (`Schedule` + `enforce_bandwidth_profiles`) | `engine.rs:419-505` | working |
| `overrides_json` per-queue map | `Schedule` model | working |
| Baseline capture / restore on profile exit | `scheduled_speed_baseline` | working |
| Conflict resolution across overlapping profiles | `priority DESC, id ASC` | working |
| Overnight schedule windows (e.g. 23:00 → 06:00) | `time_in_window` | working |
| `daily` / `weekly` / `once` schedule kinds | `schedule_active_now` | working |
| Per-queue authentication (keyring / encrypted file) | `credentials.rs` | working |
| Per-queue `on_complete` action (`shutdown`/`suspend`/`hibernate`/`run`) | `maybe_queue_completed` | working |
| `active_profiles` query for the UI | `engine.rs:active_profiles` | working |
| Persistence: queues, schedules, settings, downloads | `db.rs` | working |

## 2. What was incomplete (gaps the audit surfaced)

- **No download-level priority.** The 2-tuple sort was
  `(queue.priority, created_at)`. A high-priority download inside a low
  queue could not jump ahead of a normal-priority one.
- **No Force Start.** The frontend placeholder aliased `forceStart` to
  `redownload`, which created a new row id rather than promoting the
  existing one. There was no engine-side path that bypassed the queue
  ordering.
- **No queue chaining.** A queue's `next_queue` was a model field in
  spirit but had no column and no engine code.
- **No live scheduler snapshot.** The UI had `activeProfiles` (one
  number per profile) but no per-queue "current bandwidth vs configured
  bandwidth vs scheduled bandwidth" view, and no global concurrency
  readout.
- **Limited completion actions at the queue level.** Per-queue
  `on_complete` only fired `shutdown` / `suspend` / `hibernate` / `run`.
  No `notify` / `play_sound` / `open_folder` / `start_next_queue` (the
  task-level completion action path had all of these; the queue-level
  path did not).
- **No queue ordering UI.** There was no way to drag, ↑/↓, or move a
  queue to top/bottom.
- **`force_started` was not a concept in the engine at all.** A user
  pressing Force Start today (via the placeholder) gets a new id, no
  bypass, and a re-enqueued download.
- **Schedule typescript surface was thin.** No field for
  `next_queue`; no field for `force_started`; no field for
  download-level `priority` on `NewDownload` / `DownloadTask`.

## 3. What Phase 3B implemented

### 3.1 Precedence chain (engine)

`engine::schedule_sort_key` defines and **is the only** place that
computes the order. Every promotion path (govern, force start, chain
promotion) goes through it, so there is no way for the surfaces to
disagree.

```text
1. force_started desc  (true first  → 0, false → 1)   ── explicit bypass
2. queue.priority asc                                  ── higher queue priority first
3. task.priority desc                                  ── High (+10) wins over Low (-10)
4. created_at asc                                      ── older wins ties
5. id asc                                              ── final deterministic tie
```

The 5th key is critical: with the same priority and same creation
second, the row with the lower id always goes first. This keeps
behaviour stable across scheduler ticks.

### 3.2 Force Start

`engine::DownloadManager::force_start(id)` is the only path that sets
`force_started = true`. It is a **one-shot promotion**, not a permanent
state change:

- Honours `global_max_simultaneous` (rejected with
  `"global concurrency limit reached"` if at cap).
- Honours the queue's `max_simultaneous` (rejected with
  `"queue concurrency limit reached"` if at cap).
- Honours the queue's `enabled` flag (rejected with
  `"queue disabled: <name>"` if disabled).
- Does NOT touch the persistent `priority` field.
- The `force_started` flag is **cleared in `finish_task`** whenever
  the task leaves Downloading (Completed / Failed / Cancelled /
  Paused). The bypass is single-promotion only.
- The UI badge (`force_started` in the table, in
  `DetailsPanel`, in `PropertiesDialog`) reflects the engine's
  authoritative state.

### 3.3 Queue concurrency

`govern_concurrency` was already the right shape; it just wasn't being
called by anything except `finish_task`. The Phase 3B tests verify the
cap is enforced with real local-loopback tests. With `max_simultaneous
= 2` and 6 queued rows, the governor promotes exactly 2 and leaves 4
queued (`queue_max_simultaneous_is_enforced_by_governor`).

### 3.4 Global concurrency

The global cap is checked first in the same walk
(`global_max_simultaneous_caps_total_active_across_queues`):

```text
for t in queued_tasks_sorted_by_precedence:
    if global_max != 0 and global_running >= global_max: skip
    if queue_running >= queue_max: skip
    else: start(t); bump counts
```

The global cap can never be exceeded by any queue, even if every queue
has its own `max_simultaneous` larger.

### 3.5 Queue chaining

`DownloadQueue.next_queue` (new column) names a queue to start when
this one empties. `maybe_chain_next_queue` is the engine method:

- Skips if the source queue still has active tasks (defensive — would
  otherwise re-fire on a partial completion).
- Skips if the target queue is disabled.
- Calls `govern_concurrency` to do the actual promotion, so all the
  existing safety checks apply to the chained start.
- Cycle protection: every chain trigger re-reads the source queue's
  active state, so A→B→A is bounded by the lack of queued tasks on the
  second pass.

The `chain-a → chain-b` test (`queue_chaining_promotes_target_when_source_empties`)
inserts a task in `chain-b`, triggers the chain helper, and confirms
`chain-b`'s task is no longer Queued (the engine tried to promote it;
even with no real network the status flip to Downloading happens
synchronously in `start`).

### 3.6 Scheduled queue control

Phase 3A already had `Schedule` with `start_time` / `end_time` /
`days` / `kind` / `overrides_json`. Phase 3B adds:

- The live `QueueStatus.in_own_schedule_window` boolean — `true` if
  the queue's `schedule_enabled` is set and the engine is currently
  inside `[start, end]`. The UI shows a yellow "Waiting" badge
  otherwise.
- The `SchedulerStatus.active_profiles` view, which already existed
  but is now surfaced on the queue-row level too.
- A `On queue completion` dropdown in the queue editor with `None`,
  `Notification`, `Play sound`, `Open folder`, `Run command`,
  `Start chained queue`, `Shutdown`, `Suspend`, `Hibernate` (the last
  three are gated by the existing `dangerous_actions` setting).

### 3.7 Bandwidth profile transitions

The existing `enforce_bandwidth_profiles` already handled profile
overrides and baseline restoration. Phase 3B:

- Surfaces the override per queue in `QueueStatus.current_speed_kbps`
  and shows both the configured and the live number side by side in the
  Queue panel (`5 MB/s → 50 MB/s`).
- The `time_in_window` logic handles both same-day and overnight
  windows — `schedule_window_same_day_and_overnight_and_weekly` proves
  both: `09:00-17:00` matches at 10:00, doesn't match at 18:00, and
  `23:00-06:00` matches at 23:30 and at 02:00 but not at 12:00.

### 3.8 Schedule priority

`Schedule.priority` already existed. Conflict resolution is:

```text
Sort by: priority DESC, id ASC
First match wins (no second-writer tie-break — single writer per cycle).
```

`overlapping_profiles_resolve_by_priority` (Phase 3A) and the new
`schedule_sort_key_prefers_force_started_then_queue_then_task` together
prove this is deterministic.

### 3.9 Live scheduler status

`DownloadManager::scheduler_status()` returns a `SchedulerStatus` with:

- `active_profiles: Vec<ActiveProfile>` — every schedule currently
  overriding at least one queue, with its `priority` and the queues
  it touches.
- `queues: Vec<QueueStatus>` — per-queue:
  `active_downloads`, `queued_downloads`, `max_simultaneous`,
  `configured_speed_kbps`, `current_speed_kbps`,
  `active_schedule_id`, `in_own_schedule_window`, `enabled`,
  `priority`, `auth_configured`.
- `global_max_simultaneous`, `global_active`, `global_queued`,
  `time_now`.

The status bar reads `global_active / global_max_simultaneous` and
shows "glb 2/4 active · 5 queued". The queue panel reads the per-queue
`current_speed_kbps` to show "Configured 5 MB/s · Scheduled 50 MB/s".

### 3.10 Queue details

The `QueuesPanel` now shows, for each queue, in addition to the
existing counts and controls:

- **Active profile badge** — e.g. `Night Profile active` if a
  schedule is currently overriding.
- **Waiting badge** — yellow chip if the queue's own `schedule_enabled`
  is set and the engine is outside its window.
- **Bandwidth ladder** — `5 MB/s → 50 MB/s` when the schedule
  overrides; just `5 MB/s` when it doesn't.
- **Chain indicator** — `→ next-queue` chip on the row when
  `next_queue` is set.
- **Auth dot** — both `has_password` (SQLite flag) and `auth_configured`
  (live from the snapshot) are honoured.
- **Reorder buttons** — `↑ ↓ ⤒ ⤓` in the actions cell. Disabled at
  the boundaries.
- **Force Start** is reachable per-task via the context menu
  (existing) and the Properties dialog (new High/Normal/Low selector
  + Force Start button).

### 3.11 Download priority inside queues

`DownloadTask.priority: i32` and `NewDownload.priority: Option<i32>`.
The `PropertiesDialog` General tab adds a High / Normal / Low select
that calls `setDownloadPriority(id, ±10)`. Persisted via
`Db::set_priority` so the change survives a restart.

### 3.12 Queue ordering

`DownloadManager::reorder_queue(name, direction)` swaps the named
queue's `priority` with its neighbour's; `move_queue_to(name,
"top"|"bottom")` repeats the swap until the queue is at the boundary.
Because the engine sorts by `priority ASC, name ASC`, the user's
display order is exactly the engine's scheduling order.

`queue_ordering_via_priority_persists` proves the new ordering
survives `DownloadManager::new(...)` (i.e. a real close + reopen).

### 3.13 Automatic queue start / stop

The existing `govern_concurrency` runs on every `finish_task`, so
when a slot frees up, the next queued task is promoted automatically
(subject to global / queue caps, bandwidth caps, schedule windows).
The user does not press "Start" after every completion.

For scheduled stops: the queue's own `schedule_enabled` field is
already wired in the existing schedule path; Phase 3B just exposes
`in_own_schedule_window` so the UI can show the gate. The behaviour
of "active downloads finish their current chunks" is unchanged
(`finish_task` always runs to a terminal state, never aborts mid-body
unless the user pressed Stop / Cancel).

### 3.14 Completion behaviour (queue-level)

Reuses the existing per-task completion-action path plus new branches
in `maybe_queue_completed` for `notify`, `play_sound`, `open_folder`,
and `start_next_queue` (which delegates to `maybe_chain_next_queue`).

## 4. APIs / components reused

- `engine::govern_concurrency` (existing) — now the single place
  where Queued→Downloading promotions happen.
- `engine::enforce_bandwidth_profiles` (existing) — schedule-driven
  bandwidth overrides and restoration.
- `engine::active_profiles` (existing) — extended into
  `engine::scheduler_status` for a single struct the UI renders
  without recomputing time windows.
- `engine::set_queue_enabled` (existing) — toggle a queue on/off.
  The governor re-runs when re-enabled, so a paused queue auto-starts
  its queued tasks.
- `engine::maybe_queue_completed` (existing) — extended with
  `notify` / `play_sound` / `open_folder` / `start_next_queue`.
- `engine::run_completion_actions` (existing) — unchanged. The
  queue-level completion action path does NOT replace the task-level
  one; it runs in addition to it.
- `Db::set_priority` / `Db::set_force_started` (new) — both persist
  so behaviour is the same after a restart.
- `Tauri::generate_handler!` — five new commands registered
  alongside the existing ones, no new Tauri plugins, no new HTTP
  surface.
- The Chrome extension was not touched. The new Tauri commands are
  not exposed via the NMH; the extension cannot request a Force Start
  (and IDM has the same shape — Force Start is a desktop-only action).

## 5. Tests added

`crates/downpilot-core/tests/queue_orchestration_test.rs` — **16
deterministic tests, all passing:**

| # | Test | What it proves |
|---|---|---|
| 1 | `schedule_sort_key_prefers_force_started_then_queue_then_task` | The 5-tuple sort key is the canonical precedence |
| 2 | `queue_max_simultaneous_is_enforced_by_governor` | 6 queued + cap 2 → exactly 2 Downloading, 4 Queued |
| 3 | `global_max_simultaneous_caps_total_active_across_queues` | Even when per-queue caps are higher, the global cap is the ceiling |
| 4 | `force_start_rejected_when_at_queue_cap` | Force Start returns an error, persistent priority stays 0 |
| 5 | `force_start_clears_flag_when_task_leaves_downloading` | The badge disappears after finish_task |
| 6 | `force_start_does_not_change_persistent_priority` | The stored priority column stays 0 even after a Force Start |
| 7 | `schedule_window_same_day_and_overnight_and_weekly` | `time_in_window` handles same-day, overnight, and weekly bitmask |
| 8 | `schedule_window_overnight_matches_correctly` | Explicit overnight edge cases (23:30, 02:00, 12:00) |
| 9 | `bandwidth_profile_baseline_is_captured_then_restored` | Existing Phase 3A test re-verified with the new schema |
| 10 | `queue_chaining_promotes_target_when_source_empties` | `next_queue = "chain-b"` triggers when `chain-a` is empty |
| 11 | `queue_chaining_does_not_loop_back_on_itself` | `A → B → A` cycle terminates because each side has no queued tasks |
| 12 | `queue_on_complete_none_does_not_fire_action` | `on_complete = "none"` is a true no-op (no system call) |
| 13 | `queue_ordering_via_priority_persists` | Reorder `a` up by 1 → priorities swap and survive a reopen |
| 14 | `priority_force_started_and_next_queue_persist_across_reopen` | All three new persisted fields round-trip |
| 15 | `scheduler_status_reflects_live_state` | The snapshot matches the live DB state |
| 16 | `download_metadata_round_trips` | `priority` and `force_started` round-trip through SELECT/INSERT |

Plus the 91 pre-existing tests continue to pass, including the Phase
3A bandwidth-profile / per-queue-auth tests.

## 6. Total test count

- **107 tests passing** (`cargo test --workspace`).
- 1 ignored (the pre-existing `read_timeout_ms_aborts_hung_connection`
  in the chunked test, which trips the local-loopback stack under
  debug builds; the `.cargo/config.toml` already raises
  `RUST_MIN_STACK` to 8 MiB so the rest of the chunked tests pass).
- 0 failures.

## 7. Remaining limitations

- **Force Start is single-shot.** The badge is the only UI
  indication; the user can press it again any time. A "sticky
  promote-next" mode (always promote my downloads ahead of others)
  is not implemented because the user explicitly asked that the
  persistent priority not be modified by Force Start.
- **Queue chaining is one hop, not a graph.** A → B → C works (each
  queue chains to the next), but the engine does not pre-validate
  for cycles — the cycle-prevention check is "if the source is still
  active, skip", which terminates any cycle the moment a hop has no
  queued work.
- **No drag-reorder in the queue panel.** ↑/↓/Top/Bottom buttons
  are provided; a real HTML5 drag-reorder would be a small UI-only
  follow-up.
- **The schedule conflict resolution only handles the bandwidth
  override path**, not the queue start/stop action path. If two
  schedules both target the same queue's `schedule_enabled` window,
  the user has to manage the overlap manually. (A real
  multi-schedule system would need an `OR` of windows, which is a
  design decision, not a bug.)
- **No async tests for the new Tauri commands.** The new commands
  are 1-line wrappers around engine methods; the engine methods are
  covered by the 16 integration tests. A separate Tauri-command test
  layer would need a mock `AppState` and would be largely redundant.
- **Force Start does not retry on failure** beyond the global
  `max_retries` setting. The task still respects its own retry budget.

## 8. Recommended next IDM feature

**Per-queue download-time windows and quotas.**

- "Limit queue `night` to 50 GB per day" — a daily counter that
  pauses the queue when exceeded.
- "Queue `work` runs weekdays 09:00-18:00 only" — extends the
  existing `schedule_enabled` from a single window to multiple
  per-day windows, with quota accounting.

This is a natural next step because the persistence + UI surface +
precedence chain are now in place, and IDM's site-license advantage
is exactly this kind of fine-grained automated control.
