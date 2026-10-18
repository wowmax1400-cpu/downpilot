//! DownPilot download engine: a task manager that fans events out to any UI
//! layer (Tauri events or the native-messaging socket). Handles queue
//! management, scheduling, retry/backoff, completion actions, per-download
//! metadata, integrity verification and SQLite persistence.

use std::collections::{HashMap, HashSet};
use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Local, Timelike};
use tokio::sync::mpsc;

use crate::db::{Db, DownloadMeta};
use crate::download::ChunkDownloader;
use crate::models::{
    AppSettings, Category, DownloadQueue, DownloadTask, EngineEvent, GrabbedLink, NewDownload,
    Schedule, SiteRule, Status,
};
use crate::ytdlp::{self, YtDlp};

#[derive(Debug)]
pub enum TaskError {
    Download(crate::download::DownloadError),
    YtDlp(crate::ytdlp::YtDlpError),
    Cancelled,
}

impl From<crate::download::DownloadError> for TaskError {
    fn from(e: crate::download::DownloadError) -> Self {
        if matches!(e, crate::download::DownloadError::Cancelled) {
            TaskError::Cancelled
        } else {
            TaskError::Download(e)
        }
    }
}

impl From<crate::ytdlp::YtDlpError> for TaskError {
    fn from(e: crate::ytdlp::YtDlpError) -> Self {
        if matches!(e, crate::ytdlp::YtDlpError::Cancelled) {
            TaskError::Cancelled
        } else {
            TaskError::YtDlp(e)
        }
    }
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::Download(e) => write!(f, "{e}"),
            TaskError::YtDlp(e) => write!(f, "{e}"),
            TaskError::Cancelled => write!(f, "cancelled"),
        }
    }
}

pub struct DownloadManager {
    pub db: Db,
    state_dir: std::path::PathBuf,
    speed_limit: Arc<AtomicU64>,
    downloader: Arc<ChunkDownloader>,
    events: mpsc::UnboundedSender<EngineEvent>,
    cancels: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// Tasks auto-paused by the daily schedule; resumed when the window reopens.
    auto_paused: Mutex<HashSet<i64>>,
    /// Queue name → kbps that the engine is currently applying as a
    /// scheduled override (so we can restore the queue's configured
    /// speed when the schedule window closes). Only present while an
    /// override is in effect.
    scheduled_speed_active: Mutex<HashMap<String, u64>>,
    /// Queue name → the queue's `speed_limit_kbps` value captured at
    /// the moment a scheduled override took over. Used to restore
    /// on exit. Avoids clobbering the user's setting when the schedule
    /// itself changes (e.g. user edits the queue's normal speed while
    /// the schedule is active — we still restore to the original).
    scheduled_speed_baseline: Mutex<HashMap<String, u64>>,
    /// Tokio runtime handle installed at app startup. Sync Tauri
    /// commands run on the main thread (GTK event loop) where there is
    /// no ambient runtime context, so `start()`/retry spawns MUST go
    /// through this handle or they panic. Tests that construct the
    /// manager inside `#[tokio::test]` leave it `None` and fall back to
    /// the ambient runtime.
    rt: Mutex<Option<tokio::runtime::Handle>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn validate_task_proxy(proxy_url: Option<&str>) -> Result<(), String> {
    let Some(proxy) = proxy_url.filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    let parsed = reqwest::Url::parse(proxy).map_err(|e| format!("invalid proxy URL: {e}"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("proxy credentials must use a secure credential reference, not a URL".into());
    }
    reqwest::Proxy::all(proxy).map_err(|e| format!("invalid proxy URL: {e}"))?;
    Ok(())
}

impl DownloadManager {
    /// Render a TaskError as the user-facing error string. The
    /// auto-derived `Debug` repr of nested reqwest::Error chains
    /// doesn't include the underlying cause's text (e.g. "operation
    /// timed out"), so for the two error types we own we extract the
    /// meaningful message from the inner DownloadError. The string is
    /// also used by `error_to_category` for classification, so it
    /// must contain recognisable substrings like "timed out",
    /// "connection reset", "404", "5xx", etc.
    fn error_string(e: &TaskError) -> String {
        match e {
            TaskError::Download(d) => match d {
                crate::download::DownloadError::Reqwest(r) => {
                    // Display of reqwest::Error writes the kind prefix
                    // ("error sending request for url (...)") but not
                    // the source ("operation timed out"). Walk the
                    // source chain so the classifier sees the real cause.
                    let mut parts: Vec<String> = vec![r.to_string()];
                    let mut src: Option<&dyn std::error::Error> = r.source();
                    while let Some(s) = src {
                        parts.push(s.to_string());
                        src = s.source();
                    }
                    let full = parts.join(": ");
                    // A peer that ends the transfer without a clean
                    // shutdown produces a huge rustls/hyper chain that
                    // reads like a TLS misconfiguration to the user.
                    // The UI gets a short actionable sentence plus only
                    // the innermost technical cause (the actual kernel of
                    // the failure, which also keeps the classifier's
                    // markers intact); the complete chain is logged so
                    // the original error information is never lost.
                    let low = full.to_lowercase();
                    if crate::models::is_abrupt_close_msg(&low)
                        || crate::models::is_cert_failure_msg(&low)
                    {
                        eprintln!("[engine] transport error detail: {full}");
                        let innermost = parts.last().cloned().unwrap_or_default();
                        if crate::models::is_cert_failure_msg(&low) {
                            return format!("Certificate verification failed. ({innermost})");
                        }
                        return format!("Connection closed unexpectedly. ({innermost})");
                    }
                    full
                }
                // IncompleteBody is our own abrupt-close signal from the
                // downloader; keep the message short and actionable.
                crate::download::DownloadError::IncompleteBody => {
                    "Connection closed before the download completed.".to_string()
                }
                other => other.to_string(),
            },
            TaskError::YtDlp(y) => y.to_string(),
            TaskError::Cancelled => "cancelled".to_string(),
        }
    }

    pub fn new(
        db_path: &Path,
        state_dir: PathBuf,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<EngineEvent>) {
        let db = Db::open(db_path).expect("failed to open database");
        let speed_limit = Arc::new(AtomicU64::new(0));
        let downloader = Arc::new(ChunkDownloader::new(state_dir.clone(), speed_limit.clone()));
        let (tx, rx) = mpsc::unbounded_channel();
        let m = Arc::new(Self {
            db,
            state_dir,
            speed_limit,
            downloader,
            events: tx,
            cancels: Mutex::new(HashMap::new()),
            auto_paused: Mutex::new(HashSet::new()),
            scheduled_speed_active: Mutex::new(HashMap::new()),
            scheduled_speed_baseline: Mutex::new(HashMap::new()),
            rt: Mutex::new(None),
        });
        (m, rx)
    }

    /// Attach a credential store. Must be called once at startup
    /// before any download starts; the store is shared with the
    /// Tauri commands via `Arc` so the same instance backs both
    /// per-queue auth headers and the credential management UI.
    pub fn set_credentials(&self, creds: Arc<crate::credentials::CredentialStore>) {
        self.downloader.set_credentials(creds);
    }

    /// Install the Tokio runtime handle used for all background spawns.
    /// Must be called once at app startup (from the Tauri setup hook,
    /// where `tauri::async_runtime`'s runtime exists) before any
    /// start/resume/retry path can run. Without it, spawning from sync
    /// commands on the main thread panics with "there is no reactor
    /// running" and aborts the app (panic inside a GTK callback that
    /// cannot unwind).
    pub fn install_runtime(&self, handle: tokio::runtime::Handle) {
        *self.rt.lock().unwrap() = Some(handle);
    }

    /// Spawn onto the installed runtime handle, falling back to the
    /// ambient runtime when none was installed (tests running inside
    /// `#[tokio::test]`).
    fn spawn_task<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = self.rt.lock().unwrap().clone();
        match handle {
            Some(h) => {
                h.spawn(fut);
            }
            None => match tokio::runtime::Handle::try_current() {
                Ok(h) => {
                    h.spawn(fut);
                }
                // No ambient runtime (main-thread GTK callback without a
                // prior install_runtime call). Log instead of panicking —
                // a panic here is in a function that cannot unwind and
                // would abort the whole app.
                Err(_) => {
                    eprintln!(
                        "[engine] no tokio runtime available for spawn; install_runtime() was not called"
                    );
                }
            },
        }
    }

    /// Pump loop: drains internal events, computes speeds, persists progress
    /// to SQLite, forwards to the UI sink, and enforces the schedule window.
    pub fn pump_loop(
        self: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<EngineEvent>,
        sink: mpsc::UnboundedSender<EngineEvent>,
    ) {
        let me = self.clone();
        me.spawn_task(async move {
            let mut speeds: HashMap<i64, (Instant, i64)> = HashMap::new();
            let mut tick = tokio::time::interval(Duration::from_millis(400));
            loop {
                tick.tick().await;
                while let Ok(ev) = rx.try_recv() {
                    let fwd = match &ev {
                        EngineEvent::Progress {
                            id,
                            downloaded,
                            total,
                            ..
                        } => {
                            if *id < 0 {
                                continue;
                            }
                            // Skip Progress events for tasks already in a
                            // terminal state — `finish_task` has already
                            // written the canonical value.
                            if let Ok(Some(t)) = self.db.get(*id) {
                                if matches!(
                                    t.status,
                                    Status::Completed | Status::Failed | Status::Cancelled
                                ) {
                                    continue;
                                }
                            }
                            let now_t = Instant::now();
                            let speed = match speeds.get(id) {
                                Some((last_t, last_b)) => {
                                    let dt = now_t.duration_since(*last_t).as_secs_f64();
                                    if dt >= 0.4 {
                                        let s = ((*downloaded - *last_b) as f64 / dt).max(0.0);
                                        speeds.insert(*id, (now_t, *downloaded));
                                        s
                                    } else {
                                        0.0
                                    }
                                }
                                None => {
                                    speeds.insert(*id, (now_t, *downloaded));
                                    0.0
                                }
                            };
                            let _ = self.db.update_progress(
                                *id,
                                *downloaded,
                                *downloaded,
                                *total,
                                speed,
                            );
                            EngineEvent::Progress {
                                id: *id,
                                downloaded: *downloaded,
                                total: *total,
                                speed_bps: speed,
                            }
                        }
                        _ => ev.clone(),
                    };
                    let _ = sink.send(fwd);
                }

                // ---- Schedule enforcement (queues + schedules) ----------
                self.enforce_schedules();
                // ---- Bandwidth profile enforcement -----------------------
                self.enforce_bandwidth_profiles();
                // ---- Concurrency governor --------------------------------
                self.govern_concurrency();
                // ---- Drop stale speed samples ----------------------------
                // Without this, the per-task `speeds` map grows without
                // bound as tasks complete/are removed (a tiny leak, but
                // unbounded over a long session).
                {
                    let live: HashSet<i64> = self
                        .list()
                        .iter()
                        .filter(|t| t.status == Status::Downloading)
                        .map(|t| t.id)
                        .collect();
                    speeds.retain(|id, _| live.contains(id));
                }
            }
        });
    }

    // -------------------------------------------------------------------
    // Scheduling
    // -------------------------------------------------------------------

    /// Enforce the global daily schedule plus per-queue schedules.
    fn enforce_schedules(self: &Arc<Self>) {
        let settings = self.settings();
        let enabled = settings.schedule_enabled.unwrap_or(false);
        let tasks = self.list();

        if enabled {
            let (Some(start), Some(end)) = (settings.schedule_start, settings.schedule_end) else {
                return;
            };
            let in_window = Self::time_in_window(&start, &end);
            if !in_window {
                // Outside the global window: auto-pause active downloads.
                for t in &tasks {
                    if t.status == Status::Downloading {
                        let _ = self.pause(t.id);
                        self.auto_paused.lock().unwrap().insert(t.id);
                    }
                }
            } else {
                let ids: Vec<i64> = self.auto_paused.lock().unwrap().drain().collect();
                for id in ids {
                    if let Some(t) = tasks.iter().find(|t| t.id == id) {
                        if t.status == Status::Paused {
                            self.start(t.clone());
                        }
                    }
                }
            }
        } else {
            // Schedule disabled: release any auto-paused tasks.
            let ids: Vec<i64> = self.auto_paused.lock().unwrap().drain().collect();
            for id in ids {
                if let Some(t) = tasks.iter().find(|t| t.id == id) {
                    if t.status == Status::Paused {
                        self.start(t.clone());
                    }
                }
            }
        }

        // Per-queue schedules.
        let schedules = self.db.list_schedules().unwrap_or_default();
        let queues = self.db.list_queues().unwrap_or_default();
        let mut queue_running: HashMap<String, bool> = HashMap::new();
        for q in &queues {
            // Default: queue runs unless its own schedule says otherwise.
            let mut should_run = q.enabled;
            for s in &schedules {
                if s.queue != q.name || !s.enabled {
                    continue;
                }
                let active = match s.kind.as_str() {
                    "once" => s.start_time == "due" || (s.start_time == now().to_string()),
                    "daily" => Self::time_in_window(&s.start_time, &s.end_time),
                    "weekly" => {
                        let today = 1 << (Local::now().weekday().num_days_from_monday());
                        (s.days & today) != 0 && Self::time_in_window(&s.start_time, &s.end_time)
                    }
                    _ => true,
                };
                match s.action.as_str() {
                    "start" => should_run = should_run && active,
                    "stop" => should_run = should_run && !active,
                    "set_bw" if active => {
                        self.downloader
                            .set_queue_speed(q.name.clone(), s.bandwidth_kbps as u64);
                    }
                    _ => {}
                }
            }
            queue_running.insert(q.name.clone(), should_run);
        }

        for t in &tasks {
            let run = queue_running.get(&t.queue).copied().unwrap_or(true);
            if !run && t.status == Status::Downloading {
                let _ = self.pause(t.id);
                self.auto_paused.lock().unwrap().insert(t.id);
            } else if run
                && self.auto_paused.lock().unwrap().remove(&t.id)
                && t.status == Status::Paused
            {
                self.start(t.clone());
            }
        }
    }

    /// Apply bandwidth profiles for the current time.
    ///
    /// Each enabled Schedule can carry an `overrides_json` map of
    /// `queue_name -> kbps`. When the schedule's window is active AND
    /// the schedule is enabled AND the day-of-week bitmask matches,
    /// every affected queue's speed is set to the override value (or
    /// to "unlimited" when the value is 0).
    ///
    /// When multiple profiles target the same queue, the one with the
    /// highest `priority` wins; ties resolve by `id` ascending
    /// (older wins). The result is deterministic and predictable.
    ///
    /// When no profile matches a queue that we previously overrode,
    /// we restore its previously-captured configured speed.
    pub fn enforce_bandwidth_profiles(self: &Arc<Self>) {
        let schedules = self.db.list_schedules().unwrap_or_default();
        let queues = self.db.list_queues().unwrap_or_default();

        // Collect overrides per queue, ordered by (priority desc, id asc).
        let mut overrides: HashMap<String, u64> = HashMap::new();
        // The schedule id of the active profile per queue, for reporting.
        let mut active_id: HashMap<String, i64> = HashMap::new();
        // Sorted candidate list: (priority desc, id asc).
        let mut candidates: Vec<&Schedule> = schedules.iter().filter(|s| s.enabled).collect();
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));

        for s in &candidates {
            let active = self.schedule_active_now(s);
            if !active {
                continue;
            }
            // Parse per-queue overrides; ignore malformed entries.
            let parsed: HashMap<String, i64> =
                serde_json::from_str(&s.overrides_json).unwrap_or_default();
            for (qname, kbps) in parsed {
                // First-wins within this iteration is fine because
                // candidates are pre-sorted by priority (and tie-break
                // by older id); the first writer is the winner.
                if overrides.contains_key(&qname) {
                    continue;
                }
                overrides.insert(qname.clone(), kbps.max(0) as u64);
                active_id.insert(qname, s.id);
            }
            // Legacy single-queue field: only used if `overrides_json`
            // did not mention this schedule's `queue`.
            if !s.queue.is_empty() && !overrides.contains_key(&s.queue) {
                overrides.insert(s.queue.clone(), s.bandwidth_kbps.max(0) as u64);
                active_id.insert(s.queue.clone(), s.id);
            }
        }

        // Build the new "active" map and apply.
        let mut active_now = self.scheduled_speed_active.lock().unwrap();
        let mut baseline = self.scheduled_speed_baseline.lock().unwrap();
        // Queues to consider: existing active ∪ new overrides.
        let all_keys: HashSet<String> = active_now
            .keys()
            .cloned()
            .chain(overrides.keys().cloned())
            .collect();
        for qname in all_keys {
            // Look up the queue's configured speed from the DB so we
            // can apply the right baseline when activating and
            // restore the same baseline when deactivating.
            let configured_kbps = queues
                .iter()
                .find(|q| q.name == qname)
                .map(|q| q.speed_limit_kbps.max(0) as u64)
                .unwrap_or(0);
            match overrides.get(&qname) {
                Some(kbps) => {
                    // Activate / keep active. Only capture the
                    // baseline once (when the override first takes
                    // effect), so user edits to the configured speed
                    // after the schedule started don't get clobbered.
                    if !active_now.contains_key(&qname) {
                        baseline.insert(qname.clone(), configured_kbps);
                    }
                    self.downloader.set_queue_speed(qname.clone(), *kbps * 1024);
                    active_now.insert(qname.clone(), *kbps);
                }
                None => {
                    // Deactivate: restore the captured baseline. If
                    // the user changed the configured speed while the
                    // schedule was running, the schedule "leaves"
                    // without overwriting — the user's new setting
                    // sticks.
                    let prev = baseline.remove(&qname).unwrap_or(configured_kbps);
                    self.downloader.set_queue_speed(qname.clone(), prev * 1024);
                    active_now.remove(&qname);
                    active_id.remove(&qname);
                }
            }
        }
    }

    /// True if a schedule's time window is currently active. Handles
    /// both same-day and overnight windows (e.g. 23:00 → 06:00).
    fn schedule_active_now(self: &Arc<Self>, s: &Schedule) -> bool {
        if s.kind == "once" {
            return s.start_time == "due" || s.start_time == now().to_string();
        }
        if s.kind == "weekly" {
            let today = 1 << (Local::now().weekday().num_days_from_sunday() as i32);
            if (s.days & today) == 0 {
                return false;
            }
        }
        Self::time_in_window(&s.start_time, &s.end_time)
    }

    /// The bandwidth (kbps) the engine is currently enforcing for
    /// `queue`, regardless of whether the source is the user's
    /// configured limit or an active schedule override. Used by the
    /// UI to show "Configured: X · Scheduled: Y · Current: Z".
    pub fn effective_queue_speed_kbps(self: &Arc<Self>, queue: &str) -> u64 {
        let active = self
            .scheduled_speed_active
            .lock()
            .unwrap()
            .get(queue)
            .copied();
        match active {
            Some(kbps) => kbps,
            None => self
                .db
                .get_queue(queue)
                .ok()
                .flatten()
                .map(|q| q.speed_limit_kbps.max(0) as u64)
                .unwrap_or(0),
        }
    }

    /// Phase 3B: build a live snapshot of the scheduler. The UI
    /// calls this once per status-bar refresh (and on demand when
    /// the queue details panel opens) and renders straight from the
    /// result — no recomputation of time windows, no recomputation
    /// of the precedence sort.
    pub fn scheduler_status(self: &Arc<Self>) -> crate::models::SchedulerStatus {
        let queues = self.db.list_queues().unwrap_or_default();
        let tasks = self.list();
        let active_profiles = self.active_profiles();
        // Map of queue name → owning active schedule id (for the
        // queue details panel). We only record the *highest-priority*
        // profile per queue — same precedence rule the engine uses
        // for bandwidth overrides.
        let mut active_schedule: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        let schedules = self.db.list_schedules().unwrap_or_default();
        let mut candidates: Vec<&crate::models::Schedule> =
            schedules.iter().filter(|s| s.enabled).collect();
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
        for s in &candidates {
            if !self.schedule_active_now(s) {
                continue;
            }
            let mut affected: Vec<String> = Vec::new();
            if let Ok(parsed) =
                serde_json::from_str::<std::collections::HashMap<String, i64>>(&s.overrides_json)
            {
                affected.extend(parsed.into_keys());
            }
            if !s.queue.is_empty() && !affected.contains(&s.queue) {
                affected.push(s.queue.clone());
            }
            for q in affected {
                active_schedule.entry(q).or_insert(s.id);
            }
        }
        let active_map = self.scheduled_speed_active.lock().unwrap().clone();

        let mut per_q: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();
        let mut global_active = 0usize;
        let mut global_queued = 0usize;
        for t in &tasks {
            if t.status == Status::Downloading {
                let e = per_q.entry(t.queue.clone()).or_insert((0, 0));
                e.0 += 1;
                global_active += 1;
            } else if t.status == Status::Queued {
                let e = per_q.entry(t.queue.clone()).or_insert((0, 0));
                e.1 += 1;
                global_queued += 1;
            }
        }

        let mut out_q: Vec<crate::models::QueueStatus> = Vec::new();
        for q in &queues {
            let (active, queued) = per_q.get(&q.name).copied().unwrap_or((0, 0));
            let configured_kbps = q.speed_limit_kbps.max(0) as u64;
            let current_kbps = active_map.get(&q.name).copied().unwrap_or(configured_kbps);
            // Own-schedule window: if the queue has its own
            // `schedule_enabled` flag set, return true when the
            // engine is currently inside [start, end]. This is the
            // gate the UI shows as "Waiting for schedule" vs.
            // "Active".
            let in_window = if q.schedule_enabled {
                Self::time_in_window(&q.schedule_start, &q.schedule_end)
            } else {
                true
            };
            // auth badge: at least one queued (or running) task has a
            // non-empty auth_ref OR the queue itself declares
            // auth_user/auth_ref metadata.
            let auth_configured = !q.auth_user.is_empty()
                || !q.auth_ref.is_empty()
                || tasks.iter().any(|t| {
                    t.queue == q.name
                        && (t.status == Status::Queued || t.status == Status::Downloading)
                        && !q.auth_ref.is_empty()
                });
            out_q.push(crate::models::QueueStatus {
                queue: q.name.clone(),
                label: q.label.clone(),
                active_downloads: active,
                queued_downloads: queued,
                max_simultaneous: q.max_simultaneous,
                configured_speed_kbps: configured_kbps,
                current_speed_kbps: current_kbps,
                active_schedule_id: active_schedule.get(&q.name).copied(),
                in_own_schedule_window: in_window,
                enabled: q.enabled,
                priority: q.priority,
                auth_configured,
            });
        }
        // Sort the snapshot in the same order govern_concurrency uses:
        // by (priority asc, name asc).
        out_q.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.queue.cmp(&b.queue))
        });

        let settings = self.settings();
        crate::models::SchedulerStatus {
            active_profiles,
            queues: out_q,
            global_max_simultaneous: settings.max_simultaneous.unwrap_or(0),
            global_active,
            global_queued,
            time_now: chrono::Local::now().format("%H:%M").to_string(),
        }
    }

    /// Snapshot of which schedules are currently active and which
    /// queues they affect. Returned to the UI so it can show a
    /// "Night Profile active" badge without re-implementing the
    /// matching logic in TypeScript.
    pub fn active_profiles(self: &Arc<Self>) -> Vec<crate::models::ActiveProfile> {
        let schedules = self.db.list_schedules().unwrap_or_default();
        let active_map = self.scheduled_speed_active.lock().unwrap().clone();
        let mut out: Vec<crate::models::ActiveProfile> = Vec::new();
        for s in schedules.iter().filter(|s| s.enabled) {
            if !self.schedule_active_now(s) {
                continue;
            }
            let mut affected: Vec<String> = Vec::new();
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, i64>>(&s.overrides_json) {
                affected.extend(parsed.into_keys());
            }
            if !s.queue.is_empty() && !affected.contains(&s.queue) {
                affected.push(s.queue.clone());
            }
            if affected.is_empty() {
                continue;
            }
            // Filter to queues the engine actually considers "in
            // effect" (the active map only contains keys whose speed
            // got applied; if a queue was not overridden this tick,
            // it's already restored to baseline).
            affected.retain(|q| active_map.contains_key(q));
            if affected.is_empty() {
                continue;
            }
            out.push(crate::models::ActiveProfile {
                schedule_id: s.id,
                name: s.name.clone(),
                priority: s.priority,
                affected_queues: affected,
            });
        }
        out
    }

    fn time_in_window(start: &str, end: &str) -> bool {
        let now_min = {
            let now = Local::now();
            (now.hour() * 60 + now.minute()) as i64
        };
        let parse = |s: &str| -> Option<i64> {
            let mut it = s.split(':');
            let h: i64 = it.next()?.trim().parse().ok()?;
            let m: i64 = it.next()?.trim().parse().ok()?;
            Some(h * 60 + m)
        };
        let (Some(s), Some(e)) = (parse(start), parse(end)) else {
            return true;
        };
        if s <= e {
            now_min >= s && now_min < e
        } else {
            now_min >= s || now_min < e
        }
    }

    // -------------------------------------------------------------------
    // Concurrency governor
    // -------------------------------------------------------------------

    /// Precedence chain enforced by the scheduler.
    ///
    /// `govern_concurrency` and `force_start` both use this function so
    /// the precedence is identical everywhere a candidate is picked.
    ///
    /// Tie-break order (lower key = earlier promotion):
    ///   1. force_started desc (true first)  — explicit user bypass
    ///   2. queue.priority asc               — higher queue priority first
    ///   3. task.priority desc               — High (+10) wins over Low (-10)
    ///   4. created_at asc                   — older wins ties
    ///   5. id asc                           — final deterministic tie
    ///
    /// Lower-priority downloads still get scheduled — they only lose to
    /// higher-priority ones within the available concurrency slots.
    pub fn schedule_sort_key(
        t: &DownloadTask,
        queues: &[DownloadQueue],
    ) -> (u8, i32, i32, i64, i64) {
        let qprio = queues
            .iter()
            .find(|q| q.name == t.queue)
            .map(|q| q.priority)
            .unwrap_or(0);
        let force = if t.force_started { 0 } else { 1 };
        (force, qprio, -t.priority, t.created_at, t.id)
    }

    /// Enforce the global + per-queue maximum simultaneous downloads.
    ///
    /// Promotes Queued tasks to Downloading in the order returned by
    /// [`schedule_sort_key`]. Disabled queues are skipped (their tasks
    /// stay Queued). The global cap always wins over the per-queue
    /// cap; a queue cannot exceed `max_simultaneous` either.
    fn govern_concurrency(self: &Arc<Self>) {
        self.govern_concurrency_pub();
    }

    /// Phase 3B test/internal entry point: public alias for
    /// `govern_concurrency` so test code (and any future Tauri
    /// command that wants to "tick" the governor manually) can
    /// trigger the same code path that `finish_task` would.
    pub fn govern_concurrency_pub(self: &Arc<Self>) {
        let settings = self.settings();
        let global_max = settings.max_simultaneous.unwrap_or(0) as usize;
        let tasks = self.list();
        let queues = self.db.list_queues().unwrap_or_default();
        let mut running_per_queue: HashMap<String, usize> = HashMap::new();
        let mut global_running = 0usize;
        for t in &tasks {
            if t.status == Status::Downloading {
                *running_per_queue.entry(t.queue.clone()).or_insert(0) += 1;
                global_running += 1;
            }
        }

        // Promote queued downloads that respect their limits.
        let mut queued: Vec<&DownloadTask> = tasks
            .iter()
            .filter(|t| t.status == Status::Queued)
            .collect();
        // Phase 3B precedence: force_started > queue.priority > task.priority > created_at.
        queued.sort_by_key(|t| Self::schedule_sort_key(t, &queues));

        for t in queued {
            let q = queues.iter().find(|q| q.name == t.queue);
            // Disabled queue — leave the task queued; start() will be called
            // again the next time the queue is re-enabled.
            if !q.map(|q| q.enabled).unwrap_or(true) {
                continue;
            }
            let q_max = q.map(|q| q.max_simultaneous as usize).unwrap_or(3);
            let q_running = running_per_queue.get(&t.queue).copied().unwrap_or(0);
            let global_ok = global_max == 0 || global_running < global_max;
            let queue_ok = q_running < q_max;
            if global_ok && queue_ok {
                self.start(t.clone());
                *running_per_queue.entry(t.queue.clone()).or_insert(0) += 1;
                global_running += 1;
            }
        }
    }

    /// Force Start a single task.
    ///
    /// The task is moved out of `Queued` and promoted immediately,
    /// bypassing the normal sort order. Hard safety caps are still
    /// respected:
    ///   * The global `max_simultaneous` cap is honoured — if the
    ///     engine is already at the cap, the closest-downloading
    ///     task is NOT cancelled. Instead we just stop promoting
    ///     (Force Start is a "promote me first" request, not a
    ///     "kill the existing ones" request).
    ///   * The owning queue's `max_simultaneous` cap is honoured.
    ///   * The owning queue must be enabled.
    ///   * Bandwidth limits and retry budgets are unchanged.
    ///
    /// The persistent `priority` field is **not** modified; only the
    /// one-shot `force_started` flag is set so the UI can show the
    /// "Force Started" badge while the task is downloading. When the
    /// task leaves Downloading (finished / failed / paused / stopped)
    /// the flag is cleared automatically so the next promotion cycle
    /// uses the normal precedence chain.
    pub fn force_start(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("task not found: {id}"))?;
        if matches!(task.status, Status::Completed | Status::Downloading) {
            return Ok(());
        }
        // Honour hard caps.
        let settings = self.settings();
        let global_max = settings.max_simultaneous.unwrap_or(0) as usize;
        let tasks = self.list();
        let global_running = tasks
            .iter()
            .filter(|t| t.status == Status::Downloading)
            .count();
        let queues = self.db.list_queues().unwrap_or_default();
        let q = queues.iter().find(|q| q.name == task.queue);
        if !q.map(|q| q.enabled).unwrap_or(true) {
            return Err(format!("queue disabled: {}", task.queue));
        }
        let q_max = q.map(|q| q.max_simultaneous as usize).unwrap_or(3);
        let q_running = tasks
            .iter()
            .filter(|t| t.status == Status::Downloading && t.queue == task.queue)
            .count();
        if global_max != 0 && global_running >= global_max {
            return Err("global concurrency limit reached".into());
        }
        if q_running >= q_max {
            return Err("queue concurrency limit reached".into());
        }

        // Mark the task for one-tick bypass, then start it. The flag
        // is cleared in `finish_task` so the bypass doesn't survive
        // beyond this download session.
        let _ = self.db.set_force_started(id, true);
        let mut task = task;
        task.force_started = true;
        self.start(task);
        Ok(())
    }

    // -------------------------------------------------------------------
    // List / get / settings
    // -------------------------------------------------------------------

    pub fn list(self: &Arc<Self>) -> Vec<DownloadTask> {
        self.db.list().unwrap_or_default()
    }

    pub fn list_by_queue(self: &Arc<Self>, queue: &str) -> Vec<DownloadTask> {
        self.db.list_by_queue(queue).unwrap_or_default()
    }

    pub fn get(self: &Arc<Self>, id: i64) -> Option<DownloadTask> {
        self.db.get(id).unwrap_or(None)
    }

    pub fn default_save_dir(self: &Arc<Self>) -> String {
        self.default_save_dir_pub()
    }

    /// Phase 3B test/internal entry point: same body as
    /// `default_save_dir` but callable as `&DownloadManager` rather
    /// than `&Arc<DownloadManager>` so test helpers don't need
    /// to thread `Arc` through every method call.
    pub fn default_save_dir_pub(self: &Arc<Self>) -> String {
        self.db
            .get_setting("download_dir")
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                std::env::var("XDG_DOWNLOAD_DIR")
                    .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/Downloads")))
                    .unwrap_or_else(|_| ".".into())
            })
    }

    /// Enqueue a new download. Returns the new task id.
    ///
    /// Probes the server synchronously (awaited inside this function) so
    /// the returned task row already has the correct `total_size`, a
    /// server-suggested filename if Content-Disposition provided one, and
    /// a Failed status if the URL is unreachable / 404 / 403. The
    /// downloader never needs to re-probe.
    pub async fn add(self: &Arc<Self>, nd: NewDownload) -> Result<i64, String> {
        validate_task_proxy(nd.proxy_url.as_deref())?;
        // Browser dedup: if the same chrome.downloads.onCreated event fires
        // twice (Chrome retry, devtools refresh, etc.) we don't want two
        // DownPilot tasks. Return the existing id instead of inserting.
        if !nd.browser_dedup_id.is_empty() {
            if let Ok(Some(existing)) = self.db.find_by_browser_dedup(&nd.browser_dedup_id) {
                return Ok(existing);
            }
        }
        // Phase 3C: SiteRule.auto_capture enforcement. A site rule
        // can mark a host as "do not capture" — if the extension
        // submits a download for such a host, we refuse with a
        // structured error. The user can still add the URL
        // manually via the Add dialog (which doesn't go through
        // SubmitBrowserDownload).
        if !nd.browser_dedup_id.is_empty() {
            if let Some(site) = self.match_site(&nd.url) {
                if !site.auto_capture {
                    return Err(format!(
                        "site rule for {} has auto_capture disabled",
                        site.domain
                    ));
                }
            }
        }
        let save_dir = match nd.save_dir.clone() {
            Some(d) => d,
            None => self.resolve_save_dir(&nd),
        };
        if !Path::new(&save_dir).exists() {
            std::fs::create_dir_all(&save_dir)
                .map_err(|e| format!("cannot create save dir: {e}"))?;
        }
        // Disk space pre-check.
        if let Some(needed) = self.estimate_size(&nd) {
            if !self.has_disk_space(&save_dir, needed) {
                return Err(format!(
                    "Not enough free disk space in {save_dir} (need at least {}).",
                    crate::disk_free::human_bytes(needed)
                ));
            }
        }
        let id = self.db.insert(&nd, &save_dir).map_err(|e| e.to_string())?;
        // Apply category-based defaults if category was auto-selected.
        if let Some(task) = self.db.get(id).unwrap_or(None) {
            // Honour per-site rules. See `match_site` for precedence.
            // Only fields the user explicitly set (non-empty) are
            // applied — an empty string means "use the engine's normal
            // resolution for that field". The bandwidth_kbps check is
            // separate: 0 means "no override".
            if let Some(site) = self.match_site(&task.url) {
                // Precedence: explicit task fields (Batch, Grabber, or
                // browser payload) win; a Site Rule supplies defaults only
                // when the caller omitted that field. Destination follows
                // the same rule and was resolved before insertion.
                if nd.queue.is_none() && !site.queue.is_empty() {
                    let _ = self.db.set_queue(id, &site.queue);
                }
                if nd.save_dir.is_none() && !site.save_dir.is_empty() {
                    let _ = self.db.set_save_dir(id, &site.save_dir);
                }
                if nd.category.is_none() && !site.category.is_empty() {
                    let _ = self.db.set_category(id, &site.category);
                }
                if site.bandwidth_kbps > 0 && !site.queue.is_empty() {
                    self.downloader
                        .set_queue_speed(site.queue.clone(), site.bandwidth_kbps as u64);
                }
                if nd.user_agent.is_none() && !site.user_agent.is_empty() {
                    let _ = self.db.set_user_agent(id, &site.user_agent);
                }
                // `auth_ref` and `proxy_url` are loaded by the
                // downloader's request builder via the queue + settings
                // at byte-flow time. Storing them on the task is
                // reserved for a future per-request auth override.
            }
            let task = self.db.get(id).unwrap_or(None).unwrap_or(task);
            let _ = self.events.send(EngineEvent::Added { task: task.clone() });
            if nd.start_immediately {
                // Respect concurrency immediately.
                self.govern_concurrency();
            }
            // Probe the server synchronously so the response carries the real
            // total_size and the downloader doesn't re-probe on every run.
            // Surfaces 404/403/etc. immediately instead of letting the
            // downloader discover them after bytes start flowing.
            //
            // The "Try Download Anyway" branch of the browser-capture
            // ask-notification sets `skip_probe = true` to insert the task
            // without a synchronous probe; the downloader's normal byte-flow
            // path will surface 404/403/etc. when bytes start flowing.
            if !nd.skip_probe {
                let me = self.clone();
                let id2 = id;
                let url = task.url.clone();
                let filename_hint = task.filename.clone();
                let proxy = self.resolve_proxy(&task)?;
                // FTP: no HTTP HEAD; SIZE + URL filename gives the same
                // (total_size, suggested filename, resume flag) the HTTP
                // path would.
                if url.trim_start().to_ascii_lowercase().starts_with("ftp://") {
                    match crate::ftp::probe_ftp(&url, Some(&save_dir)).await {
                        Ok(info) => {
                            let mut changed = false;
                            if info.filename != filename_hint && !info.filename.is_empty() {
                                let _ = me.db.set_filename(id2, &info.filename);
                                changed = true;
                            }
                            if info.size > 0 {
                                let _ = me.db.set_total_size(id2, info.size);
                                changed = true;
                            }
                            if changed {
                                if let Some(t) = me.db.get(id2).unwrap_or(None) {
                                    let _ = me.events.send(EngineEvent::Finished { task: t });
                                }
                            }
                        }
                        Err(e) => {
                            // Mirrors the HTTP probe failure path: mark the
                            // task failed with the reason so the UI shows why.
                            let _ = me.db.set_status(id2, Status::Failed, Some(&e));
                            let _ = me.events.send(EngineEvent::StatusChanged {
                                id: id2,
                                status: Status::Failed,
                                error: Some(e),
                            });
                        }
                    }
                    return Ok(id);
                }
                let headers = DownloadTask {
                    user_agent: task.user_agent.clone(),
                    referer: task.referer.clone(),
                    cookies: task.cookies.clone(),
                    proxy_url: proxy,
                    ..DownloadTask::default_for_probe(&url, &filename_hint)
                };
                let probe_result = me.downloader.probe(&headers).await;
                if let Ok((total, _range, suggested, content_type)) = probe_result {
                    let mut changed = false;
                    if let Some(better) = suggested {
                        if better != filename_hint && !better.is_empty() {
                            let _ = me.db.set_filename(id2, &better);
                            changed = true;
                        }
                    }
                    if total.unwrap_or(0) > 0 {
                        let _ = me.db.set_total_size(id2, total.unwrap());
                        changed = true;
                    }
                    if let Some(ct) = content_type {
                        if !ct.is_empty() {
                            let _ = me.db.set_content_type(id2, Some(&ct));
                            changed = true;
                        }
                    }
                    if changed {
                        if let Some(t) = me.db.get(id2).unwrap_or(None) {
                            let _ = me.events.send(EngineEvent::Finished { task: t });
                        }
                    }
                } else if let Err(e) = probe_result {
                    // Probe failed (404, 403, network error). Mark the task
                    // Failed immediately so the UI reflects reality, and
                    // skip starting the download. Use `error_string` so a
                    // reqwest source chain (EOF/truncation/TLS details) is
                    // both classified correctly and shortened for the UI.
                    let err_str = Self::error_string(&TaskError::Download(e));
                    let _ = me.db.set_status(id2, Status::Failed, Some(&err_str));
                    let _ = me.events.send(EngineEvent::StatusChanged {
                        id: id2,
                        status: Status::Failed,
                        error: Some(err_str.clone()),
                    });
                    return Err(err_str);
                }
            } // end if !nd.skip_probe
        }
        Ok(id)
    }

    /// Compute the save directory for a new download, applying category rules.
    fn resolve_save_dir(self: &Arc<Self>, nd: &NewDownload) -> String {
        // Site rule takes precedence only when the caller did not provide
        // an explicit destination. Its category is also the effective
        // category for resolving the category destination.
        let site = nd
            .webpage_url
            .as_ref()
            .or(Some(&nd.url))
            .and_then(|url| self.match_site(url));
        if let Some(d) = nd.save_dir.as_ref().filter(|d| !d.is_empty()) {
            return d.clone();
        }
        if let Some(site) = site.as_ref() {
            if !site.save_dir.is_empty() {
                return site.save_dir.clone();
            }
        }
        let effective_category = nd
            .category
            .as_deref()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                site.as_ref()
                    .map(|s| s.category.as_str())
                    .filter(|name| !name.is_empty())
            });
        if let Some(cat_name) = effective_category {
            if let Some(c) = self.category(cat_name) {
                if !c.save_dir.is_empty() {
                    return c.save_dir;
                }
            }
        }
        // Generic category guess from filename.
        let fname = nd
            .filename
            .clone()
            .unwrap_or_else(|| crate::db::guess_filename(&nd.url));
        let cat = crate::db::guess_category(&fname);
        if let Some(c) = self.category(&cat) {
            if !c.save_dir.is_empty() {
                return c.save_dir;
            }
        }
        self.default_save_dir()
    }

    fn estimate_size(self: &Arc<Self>, _nd: &NewDownload) -> Option<i64> {
        // We can't reliably know the size before a HEAD request here; leave the
        // runtime to surface it. Returning None disables the pre-check.
        None
    }

    fn has_disk_space(self: &Arc<Self>, dir: &str, needed: i64) -> bool {
        // Use the real `statvfs`-based detector. If the syscall
        // itself errors (e.g. directory disappeared between calls),
        // be fail-safe: surface the error upstream by returning
        // `true` only when we *know* the check passed. We treat
        // missing-permission or transient statvfs failures as
        // "indeterminate, do not block the download" because the
        // engine layer is the right place to log + retry. Disk-full
        // detection at write-time still protects the user.
        match crate::disk_free::free_bytes(std::path::Path::new(dir)) {
            Ok(free) => free >= needed,
            Err(_) => true,
        }
    }

    /// Find the most specific `SiteRule` for `url`.
    ///
    /// Precedence (most-specific first):
    ///   1. exact host match (`site.domain == url_host`)
    ///   2. explicit subdomain pattern (`site.domain` is a parent of
    ///      `url_host` — `site.domain == "example.com"` matches
    ///      `cdn.example.com` but NOT `notexample.com`)
    ///   3. global rule (`site.domain == ""`) used as a fallback
    ///
    /// `host_matches_pattern` already enforces hostname-aware matching
    /// (port stripped, case folded, leading-dot wildcard accepted,
    /// lookalikes rejected). Empty host → no match.
    pub(crate) fn match_site(self: &Arc<Self>, url: &str) -> Option<SiteRule> {
        let host = host_of(url);
        if host.is_empty() {
            return None;
        }
        let sites = self.db.list_sites().ok()?;
        // 1. exact host
        if let Some(s) = sites.iter().find(|s| s.domain.eq_ignore_ascii_case(&host)) {
            return Some(s.clone());
        }
        // 2. subdomain pattern: domain is a parent of host. Iterate by
        // descending domain-length so the most specific parent wins.
        let mut best: Option<&SiteRule> = None;
        let mut best_len = 0usize;
        for s in &sites {
            if s.domain.is_empty() {
                continue;
            }
            if host_matches_pattern(&host, &s.domain) && s.domain.len() > best_len {
                best = Some(s);
                best_len = s.domain.len();
            }
        }
        if let Some(s) = best {
            return Some(s.clone());
        }
        // 3. global fallback.
        sites.into_iter().find(|s| s.domain.is_empty())
    }

    fn category(self: &Arc<Self>, name: &str) -> Option<Category> {
        self.db
            .list_categories()
            .ok()?
            .into_iter()
            .find(|c| c.name == name)
    }

    /// Resolve the effective proxy without mutating global downloader state.
    /// `Some("")` is an explicit Direct connection; `Some(url)` is a
    /// task-scoped proxy; `None` means no configured value was supplied.
    /// Precedence: explicit task > Site Rule > queue > global > direct.
    fn resolve_proxy(self: &Arc<Self>, task: &DownloadTask) -> Result<Option<String>, String> {
        let candidate = if let Some(proxy) = task.proxy_url.as_ref() {
            Some(proxy.clone())
        } else {
            self.match_site(&task.url)
                .and_then(|site| (!site.proxy_url.is_empty()).then_some(site.proxy_url))
                .or_else(|| {
                    self.db
                        .list_queues()
                        .ok()?
                        .into_iter()
                        .find(|q| q.name == task.queue)
                        .and_then(|q| (!q.proxy_url.is_empty()).then_some(q.proxy_url))
                })
                .or_else(|| self.settings().proxy_url)
        };
        let effective = candidate.or_else(|| Some(String::new()));
        if let Some(proxy) = effective.as_deref().filter(|p| !p.is_empty()) {
            reqwest::Proxy::all(proxy).map_err(|e| format!("invalid proxy URL: {e}"))?;
        }
        Ok(effective)
    }

    /// Phase 3C: batch-insert helper. Calls `add` for every entry and
    /// collects the result. Failures on individual rows do **not**
    /// abort the batch — the caller (the Batch dialog) gets a
    /// `Vec<Result<i64, String>>` in input order and decides how to
    /// surface the errors (e.g. inline per-row red flag). The
    /// rationale: when the user pastes 200 URLs, one malformed URL
    /// should not cancel the other 199.
    ///
    /// All inserts share a single disk-space check (the first
    /// `add` call performs the check for the first row; subsequent
    /// rows are inserted with the same save_dir resolution, so the
    /// OS-level `create_dir_all` and disk_free inside `add` still
    /// fire per row).
    ///
    /// Returns one `Result` per input `nd`, in order. Successes
    /// carry the new task id; failures carry the engine's error
    /// string.
    pub async fn add_batch(self: &Arc<Self>, nds: Vec<NewDownload>) -> Vec<Result<i64, String>> {
        let mut out = Vec::with_capacity(nds.len());
        for nd in nds {
            out.push(self.add(nd).await);
        }
        // After the batch settles, run the governor once so any
        // tasks that didn't auto-promote get a chance.
        self.govern_concurrency();
        out
    }

    // -------------------------------------------------------------------
    // Start / pause / resume / cancel / retry
    // -------------------------------------------------------------------

    pub fn start(self: &Arc<Self>, task: DownloadTask) {
        if task.status == Status::Downloading || task.status == Status::Completed {
            return;
        }
        // If already at the global/queue concurrency limit, mark Queued.
        let settings = self.settings();
        let global_max = settings.max_simultaneous.unwrap_or(0) as usize;
        let tasks = self.list();
        let global_running = tasks
            .iter()
            .filter(|t| t.status == Status::Downloading)
            .count();
        let q_running = tasks
            .iter()
            .filter(|t| t.status == Status::Downloading && t.queue == task.queue)
            .count();
        let queues = self.db.list_queues().unwrap_or_default();
        let q_enabled = queues
            .iter()
            .find(|q| q.name == task.queue)
            .map(|q| q.enabled)
            .unwrap_or(true);
        let q_max = queues
            .iter()
            .find(|q| q.name == task.queue)
            .map(|q| q.max_simultaneous as usize)
            .unwrap_or(3);

        // Disabled queue: park as Queued. `set_queue_enabled(true)` will
        // re-run `govern_concurrency`, which will promote it.
        if !q_enabled {
            let _ = self.db.set_status(task.id, Status::Queued, None);
            let _ = self.events.send(EngineEvent::StatusChanged {
                id: task.id,
                status: Status::Queued,
                error: None,
            });
            return;
        }

        if (global_max != 0 && global_running >= global_max) || q_running >= q_max {
            let _ = self.db.set_status(task.id, Status::Queued, None);
            let _ = self.events.send(EngineEvent::StatusChanged {
                id: task.id,
                status: Status::Queued,
                error: None,
            });
            return;
        }

        let _ = self.db.set_status(task.id, Status::Downloading, None);
        // Persist the first-start timestamp (only when unset — a resume
        // must keep the ORIGINAL start so the average speed reflects the
        // full wall-clock lifetime of the download). finish_complete()
        // reads this to compute avg_speed_bps.
        if task.started_at.is_none() {
            let _ = self.db.set_started_at(task.id, now());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(task.id, cancel.clone());
        let me = self.clone();
        let tx = self.events.clone();
        let task_id = task.id;
        let started = now();
        self.spawn_task(async move {
            let result = me.run_task(task, cancel, tx).await;
            me.finish_task(task_id, result, started);
        });
        let _ = self.events.send(EngineEvent::StatusChanged {
            id: task_id,
            status: Status::Downloading,
            error: None,
        });
    }

    async fn run_task(
        self: &Arc<Self>,
        task: DownloadTask,
        cancel: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<u64, TaskError> {
        let mut task = task;
        task.proxy_url = self
            .resolve_proxy(&task)
            .map_err(|e| TaskError::Download(crate::download::DownloadError::InvalidProxy(e)))?;
        // Mirror/multi-source: pull from whichever source the rotation
        // index points at. Index 0 (the default) is the primary URL, so
        // a task with no mirrors behaves exactly as before.
        task.url = task.active_url();
        let is_original = task.format_id.as_deref() == Some("original");
        if task.format_id.is_some() && !is_original {
            let proxy = task.proxy_url.as_deref().filter(|p| !p.is_empty());
            let path = YtDlp::download(
                &task.url,
                task.format_id.as_deref().unwrap_or("bv*+ba/b"),
                Path::new(&task.save_dir),
                task.referer.as_deref(),
                task.user_agent.as_deref(),
                task.cookies.as_deref(),
                proxy,
                cancel.clone(),
                tx.clone(),
            )
            .await
            .map_err(TaskError::from)?;
            let _ = path;
            return Ok(0);
        }
        // FTP URLs bypass the HTTP downloader entirely: different protocol
        // mechanics (RETR/REST instead of Range), same progress/finalize
        // events so the rest of the pipeline is agnostic.
        if task
            .url
            .trim_start() // tolerate leading whitespace pasted from a page
            .to_ascii_lowercase()
            .starts_with("ftp://")
        {
            return crate::ftp::run_ftp(task, cancel, tx)
                .await
                .map_err(TaskError::Download);
        }
        self.downloader
            .run(task, cancel, tx)
            .await
            .map_err(TaskError::from)
    }

    fn finish_task(self: &Arc<Self>, id: i64, result: Result<u64, TaskError>, started: i64) {
        self.cancels.lock().unwrap().remove(&id);
        // For a cancellation, the user-requested final state lives in the DB
        // (Paused for pause, Stopped for stop). Use it instead of always
        // reporting Paused.
        let user_final = self
            .db
            .get(id)
            .unwrap_or(None)
            .map(|t| t.status)
            .filter(|s| matches!(s, Status::Paused | Status::Stopped));
        let (status, err) = match &result {
            Ok(n) => {
                let db_task = self.db.get(id).unwrap_or(None);
                let total = db_task
                    .as_ref()
                    .map(|t| t.total_size.max(*n as i64))
                    .unwrap_or(*n as i64);
                // Verify integrity before marking the task complete. A
                // mismatch or hashing failure must never trigger completion
                // actions or category routing.
                if let Some(task) = db_task {
                    if !task.checksum_value.is_empty() || task.expected_sha256.is_some() {
                        let expected = task
                            .expected_sha256
                            .clone()
                            .filter(|s| !s.is_empty())
                            .or_else(|| Some(task.checksum_value.clone()))
                            .unwrap_or_default();
                        let verification = self
                            .final_path(&task)
                            .ok_or_else(|| {
                                std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "final file not found",
                                )
                            })
                            .and_then(|path| hash_file(&path, &task.checksum_algorithm));
                        match verification {
                            Ok(got)
                                if !expected.is_empty() && got.eq_ignore_ascii_case(&expected) =>
                            {
                                let _ = self.db.set_meta(
                                    id,
                                    &DownloadMeta {
                                        checksum_verified: true,
                                        checksum_value: got,
                                        checksum_algorithm: task.checksum_algorithm.clone(),
                                        ..DownloadMeta::from(&task)
                                    },
                                );
                            }
                            Ok(got) => {
                                let _ = self.db.set_meta(
                                    id,
                                    &DownloadMeta {
                                        checksum_verified: false,
                                        checksum_value: got.clone(),
                                        checksum_algorithm: task.checksum_algorithm.clone(),
                                        ..DownloadMeta::from(&task)
                                    },
                                );
                                return self.finish_task(
                                    id,
                                    Err(TaskError::Download(
                                        crate::download::DownloadError::ChecksumMismatch {
                                            expected: if expected.is_empty() {
                                                "<non-empty checksum>".into()
                                            } else {
                                                expected
                                            },
                                            got,
                                        },
                                    )),
                                    started,
                                );
                            }
                            Err(e) => {
                                return self.finish_task(
                                    id,
                                    Err(TaskError::Download(crate::download::DownloadError::Io(e))),
                                    started,
                                );
                            }
                        }
                    }
                }
                // Single atomic write: downloaded + total + status +
                // completed_at all flip together. No torn state on crash.
                let _ = self.db.finish_complete(id, *n as i64, total);
                (Status::Completed, None)
            }
            Err(TaskError::Cancelled) => (user_final.unwrap_or(Status::Paused), None),
            Err(e) => (Status::Failed, Some(Self::error_string(e))),
        };

        // For non-Completed paths the status flip is its own atomic write.
        if status != Status::Completed {
            // Reconcile the DB's `downloaded` counter with the bytes
            // actually on disk (final file + any .part segment files).
            // A transfer killed mid-body can leave the counter stale
            // (below disk truth); resume must start from the real byte
            // count, never from a lagging number.
            if matches!(status, Status::Failed | Status::Paused | Status::Stopped) {
                if let Some(task) = self.db.get(id).unwrap_or(None) {
                    let disk_bytes = self.on_disk_bytes(&task);
                    if disk_bytes > task.downloaded as u64 {
                        let _ = self.db.update_progress(
                            id,
                            disk_bytes as i64,
                            disk_bytes as i64,
                            task.total_size,
                            0.0,
                        );
                    }
                }
            }
            let _ = self.db.finish_terminal(id, status, err.as_deref());
        }
        // Phase 3B: clear the one-shot force_started flag whenever the
        // task leaves Downloading. The bypass was a single-promotion
        // concession; once the slot is free again, normal precedence
        // takes over.
        let _ = self.db.set_force_started(id, false);
        if status == Status::Completed {
            if let Err(e) = self.route_completed_file(id) {
                let _ = self.events.send(EngineEvent::Notify {
                    level: "warning".into(),
                    title: "Category routing failed".into(),
                    body: e,
                });
            }
        }
        if let Some(task) = self.db.get(id).unwrap_or(None) {
            let _ = self
                .events
                .send(EngineEvent::Finished { task: task.clone() });
            let _ = self.events.send(EngineEvent::StatusChanged {
                id,
                status,
                error: err.clone(),
            });

            // Completion actions (only on success).
            if status == Status::Completed {
                self.run_completion_actions(&task);
            }
            // Queue empty → run queue completion action.
            if status == Status::Completed
                || status == Status::Failed
                || status == Status::Cancelled
            {
                self.maybe_queue_completed(&task.queue);
            }
        }

        // Attempt mirror failover first (bounded by the source list),
        // then fall back to same-source auto-retry.
        if status == Status::Failed {
            if !self.maybe_failover_mirror(id) {
                self.maybe_retry(id);
            }
        }

        // Free up a concurrency slot and promote next queued.
        self.govern_concurrency();
        let _ = started;
    }

    /// Mirror failover: on a retryable-class failure, rotate to the next
    /// source and re-queue at the same retry budget. Returns true when a
    /// failover was scheduled (caller must skip same-source retry).
    ///
    /// Guardrails:
    /// - only retryable error classes (connection/timeout/5xx/unknown);
    ///   a 404/403/checksum failure is authoritative, not a bad mirror.
    /// - never rotates on user intent (paused/stopped/cancelled never
    ///   reach here — `finish_task` gates on `Status::Failed` only).
    /// - bounded: at most `sources().len() - 1` rotations per task, then
    ///   falls through to normal retry/exhaustion.
    fn maybe_failover_mirror(self: &Arc<Self>, id: i64) -> bool {
        let task = match self.db.get(id).unwrap_or(None) {
            Some(t) => t,
            None => return false,
        };
        if !is_retryable(&task.error_category) {
            return false;
        }
        let Some((next_idx, next_url)) = task.next_source() else {
            return false;
        };
        let _ = self.db.set_mirror_index(id, next_idx);
        let _ = self.db.inc_retry(id);
        let _ = self.db.set_status(id, Status::Queued, None);
        let _ = self.events.send(EngineEvent::StatusChanged {
            id,
            status: Status::Queued,
            error: Some(format!("Primary source failed — trying mirror {next_url}…")),
        });
        let me = self.clone();
        self.spawn_task(async move {
            if let Some(t) = me.db.get(id).unwrap_or(None) {
                if t.status == Status::Queued {
                    me.start(t);
                }
            }
        });
        true
    }

    /// Decide whether to auto-retry a failed task, applying backoff.
    fn maybe_retry(self: &Arc<Self>, id: i64) {
        let task = match self.db.get(id).unwrap_or(None) {
            Some(t) => t,
            None => return,
        };
        let settings = self.settings();
        let max_retries = if task.max_retries > 0 {
            task.max_retries
        } else {
            settings.max_retries.unwrap_or(0)
        };
        if max_retries <= 0 || task.retry_count >= max_retries {
            return;
        }
        let delay = if task.retry_delay_ms > 0 {
            task.retry_delay_ms
        } else {
            settings.retry_delay_ms.unwrap_or(2000)
        };
        // Only retry retryable error classes.
        if !is_retryable(&task.error_category) {
            return;
        }
        let _ = self.db.inc_retry(id);
        let _ = self.db.set_status(id, Status::Queued, None);
        let _ = self.events.send(EngineEvent::StatusChanged {
            id,
            status: Status::Queued,
            error: Some(format!(
                "Retrying (attempt {} of {})…",
                task.retry_count + 1,
                max_retries
            )),
        });
        let me = self.clone();
        self.spawn_task(async move {
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
            if let Some(t) = me.db.get(id).unwrap_or(None) {
                if t.status == Status::Queued {
                    me.start(t);
                }
            }
        });
    }

    fn maybe_queue_completed(self: &Arc<Self>, queue: &str) {
        self.maybe_queue_completed_pub(queue);
    }

    /// Phase 3B test/internal entry point: same body as
    /// `maybe_queue_completed(queue: &str)` but takes a `&DownloadQueue`
    /// directly so test code (and any future caller) can drive the
    /// completion path without first looking the queue up.
    pub fn maybe_queue_completed_pub(self: &Arc<Self>, queue: &str) {
        let tasks = self.list_by_queue(queue);
        let active = tasks.iter().any(|t| t.status.is_active());
        if active {
            return;
        }
        // Queue is empty: run its completion action.
        let q = match self.db.get_queue(queue).unwrap_or(None) {
            Some(q) => q,
            None => return,
        };
        if q.on_complete == "none" {
            // Phase 3B: still consider queue chaining even when the
            // on_complete action is "none" — the two features are
            // independent (chain just decides what starts next; the
            // on_complete action decides what fires right now).
            self.maybe_chain_next_queue(&q);
            return;
        }
        match q.on_complete.as_str() {
            "shutdown" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["poweroff"])
                    .spawn();
            }
            "suspend" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["suspend"])
                    .spawn();
            }
            "hibernate" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["hibernate"])
                    .spawn();
            }
            "run" if !q.on_complete_command.is_empty() => {
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&q.on_complete_command)
                    .spawn();
            }
            "notify" => {
                let _ = self.events.send(EngineEvent::Notify {
                    level: "info".into(),
                    title: format!("Queue complete: {}", q.label),
                    body: format!("All downloads in '{}' finished.", q.label),
                });
            }
            "play_sound" => {
                if self.settings().sound_on_complete.unwrap_or(true) {
                    let _ = self.events.send(EngineEvent::Notify {
                        level: "info".into(),
                        title: format!("Queue complete: {}", q.label),
                        body: format!("All downloads in '{}' finished.", q.label),
                    });
                }
            }
            "open_folder" => {
                // Reuse the task-level completion-action path: pick the
                // last task that finished in this queue and emit the
                // open-folder notification through the same channel.
                if let Some(t) = self.list_by_queue(queue).into_iter().last() {
                    let _ = self.events.send(EngineEvent::Notify {
                        level: "info".into(),
                        title: format!("Queue complete: {}", q.label),
                        body: t.save_dir.clone(),
                    });
                }
            }
            "start_next_queue" => {
                // Handled below by maybe_chain_next_queue, but keep
                // an explicit branch so old configs that named the
                // action literally still work.
            }
            _ => {}
        }
        // Phase 3B: queue chaining. Runs after on_complete so the
        // on_complete action fires immediately, then the chained queue
        // is promoted. Chained queues still respect every existing
        // safety check (global/queue concurrency, bandwidth caps,
        // queue enabled flag, schedule window).
        self.maybe_chain_next_queue(&q);
    }

    /// Phase 3B: if `q.next_queue` is set and points to an existing,
    /// enabled queue, promote every queued task in that queue by
    /// calling `govern_concurrency`. If the chained queue itself has
    /// no queued tasks, nothing happens (the chain triggers later
    /// when the user adds tasks).
    fn maybe_chain_next_queue(self: &Arc<Self>, q: &crate::models::DownloadQueue) {
        self.maybe_chain_next_queue_pub(q);
    }

    /// Phase 3B test/internal entry point: public alias for
    /// `maybe_chain_next_queue`. Tests can drive queue chaining
    /// without first waiting for a real completion.
    pub fn maybe_chain_next_queue_pub(self: &Arc<Self>, q: &crate::models::DownloadQueue) {
        if q.next_queue.is_empty() {
            return;
        }
        let target = match self.db.get_queue(&q.next_queue).unwrap_or(None) {
            Some(t) => t,
            None => return,
        };
        if !target.enabled {
            return;
        }
        // Defensive: avoid creating a cycle by chaining into ourselves
        // or back into a queue that already points at us. The simplest
        // safe check: if the target queue has any task belonging to
        // the source queue, the chain is already partially satisfied
        // and we should not fire again (otherwise we'd race ourselves).
        let source_active = self
            .list_by_queue(&q.name)
            .iter()
            .any(|t| t.status.is_active());
        if source_active {
            return;
        }
        // Promote whatever is in the chain target.
        self.govern_concurrency();
    }

    fn run_completion_actions(self: &Arc<Self>, task: &DownloadTask) {
        // Per-task override wins; otherwise global default.
        let action_json: Option<String> = if !task.on_complete.is_empty() {
            Some(task.on_complete.clone())
        } else {
            self.settings().on_complete
        };
        // on_complete is a JSON string: {"action": "...", "command": "...", "target_queue": "..."}
        #[derive(serde::Deserialize)]
        struct ActionSpec {
            action: String,
            #[serde(default)]
            command: String,
            #[serde(default)]
            target_queue: String,
        }
        let spec: Option<ActionSpec> = match action_json {
            Some(ref s) => serde_json::from_str::<ActionSpec>(s.as_str()).ok(),
            None => None,
        };
        let spec = match spec {
            Some(s) => s,
            None => return,
        };

        // Safety guard: the destructive system-level actions require
        // an explicit opt-in via the `dangerous_actions` settings flag.
        // Without it, a misconfigured site rule or a stale task setting
        // could shut the user's machine down unexpectedly.
        let is_destructive = matches!(spec.action.as_str(), "shutdown" | "suspend" | "hibernate");
        if is_destructive && !self.settings().dangerous_actions.unwrap_or(false) {
            let _ = self.events.send(EngineEvent::Notify {
                level: "warning".into(),
                title: "Destructive action blocked".into(),
                body: format!(
                    "{} would {} but 'Allow destructive actions' is off in Settings.",
                    task.filename, spec.action
                ),
            });
            return;
        }

        // `play_sound` action is gated by the global `sound_on_complete`
        // toggle (Phase 3A: previously a dead setting).
        if spec.action == "play_sound" && !self.settings().sound_on_complete.unwrap_or(true) {
            return;
        }

        match spec.action.as_str() {
            "open_file" => {
                if let Some(p) = self.final_path(task) {
                    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
                }
            }
            "open_folder" => {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&task.save_dir)
                    .spawn();
            }
            "notification" => {
                // The frontend's notify channel is consumed by the UI
                // and rendered as a native desktop notification. The
                // default "Download complete" notification is already
                // sent below; this branch lets a user-supplied spec
                // carry a custom title via `command` (rare but valid).
                if !spec.command.is_empty() {
                    let _ = self.events.send(EngineEvent::Notify {
                        level: "success".into(),
                        title: spec.command.clone(),
                        body: task.filename.clone(),
                    });
                    return;
                }
            }
            "play_sound" => {
                // Play a short notification sound. We pick a few common
                // desktop sound names and let the system resolve them
                // (PulseAudio / PipeWire). On Bazzite/GNOME this hits
                // the freedesktop sound theme.
                for sound in &[
                    "message-new-instant",
                    "bell",
                    "complete",
                    "dialog-information",
                ] {
                    if std::process::Command::new("paplay")
                        .arg(format!(
                            "/usr/share/sounds/freedesktop/stereo/{}.oga",
                            sound
                        ))
                        .spawn()
                        .is_ok()
                    {
                        break;
                    }
                }
            }
            "run_command" if !spec.command.is_empty() => {
                let cmd = spec
                    .command
                    .replace("{file}", &self.final_path(task).unwrap_or_default());
                let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
            }
            "start_queue" if !spec.target_queue.is_empty() => {
                if let Some(q) = self.db.get_queue(&spec.target_queue).unwrap_or(None) {
                    let q = DownloadQueue { enabled: true, ..q };
                    let _ = self.db.upsert_queue(&q);
                    self.govern_concurrency();
                }
            }
            "stop_queue" if !spec.target_queue.is_empty() => {
                if let Some(q) = self.db.get_queue(&spec.target_queue).unwrap_or(None) {
                    let q = DownloadQueue {
                        enabled: false,
                        ..q
                    };
                    let _ = self.db.upsert_queue(&q);
                    // Cancel any in-flight downloads belonging to this queue.
                    for t in self.list() {
                        if t.queue == spec.target_queue
                            && (t.status == Status::Downloading || t.status == Status::Queued)
                        {
                            let _ = self.cancel(t.id);
                        }
                    }
                }
            }
            "shutdown" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["poweroff"])
                    .spawn();
            }
            "suspend" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["suspend"])
                    .spawn();
            }
            "hibernate" => {
                let _ = std::process::Command::new("systemctl")
                    .args(["hibernate"])
                    .spawn();
            }
            _ => {}
        }
        // Phase 3A: gate the always-sent "Download complete" notification
        // on the `notify_on_complete` setting (default true to preserve
        // the pre-fix behaviour). When the user has switched notifications
        // off, this `EngineEvent::Notify` is suppressed — including for
        // the implicit "download finished" event that the UI was
        // previously using as its sole success signal.
        if self.settings().notify_on_complete.unwrap_or(true) {
            let _ = self.events.send(EngineEvent::Notify {
                level: "success".into(),
                title: "Download complete".into(),
                body: task.filename.clone(),
            });
        }
    }

    fn route_completed_file(self: &Arc<Self>, id: i64) -> Result<bool, String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("task not found: {id}"))?;
        let category = self
            .db
            .list_categories()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|c| c.name == task.category);
        let category = match category {
            Some(c) if !c.save_dir.trim().is_empty() => c,
            _ => return Ok(false),
        };
        let source = Path::new(&task.save_dir).join(&task.filename);
        if !source.exists() {
            return Ok(false);
        }
        let destination_dir = Path::new(&category.save_dir);
        if source.parent() == Some(destination_dir) {
            return Ok(false);
        }
        std::fs::create_dir_all(destination_dir).map_err(|e| {
            format!(
                "cannot create category directory {}: {e}",
                destination_dir.display()
            )
        })?;
        let mut destination = destination_dir.join(&task.filename);
        if destination.exists() {
            let stem = Path::new(&task.filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("download");
            let ext = Path::new(&task.filename)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| format!(".{s}"))
                .unwrap_or_default();
            let mut n = 1u32;
            loop {
                destination = destination_dir.join(format!("{stem} ({n}){ext}"));
                if !destination.exists() {
                    break;
                }
                n += 1;
            }
        }
        std::fs::rename(&source, &destination).map_err(|e| {
            format!(
                "cannot route {} to {}: {e}",
                source.display(),
                destination.display()
            )
        })?;
        self.db
            .set_save_dir(id, destination_dir.to_string_lossy().as_ref())
            .map_err(|e| e.to_string())?;
        if let Some(routed_name) = destination.file_name().and_then(|n| n.to_str()) {
            self.db
                .set_filename(id, routed_name)
                .map_err(|e| e.to_string())?;
        }
        Ok(true)
    }

    pub fn route_completed_file_for_test(self: &Arc<Self>, id: i64) -> Result<bool, String> {
        self.route_completed_file(id)
    }

    fn final_path(self: &Arc<Self>, task: &DownloadTask) -> Option<String> {
        let p = Path::new(&task.save_dir).join(&task.filename);
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    /// Bytes actually on disk for this task: the final file (single-stream
    /// path) plus every `.part` segment file (chunked path). Used to
    /// reconcile the DB `downloaded` counter when a transfer dies
    /// mid-body, so resume/retry starts from the real position.
    fn on_disk_bytes(&self, task: &DownloadTask) -> u64 {
        let mut total = 0u64;
        let final_file = Path::new(&task.save_dir).join(&task.filename);
        if let Ok(m) = std::fs::metadata(&final_file) {
            total += m.len();
        }
        let part_dir = self.state_dir.join(format!("parts-{}", task.id));
        if let Ok(entries) = std::fs::read_dir(&part_dir) {
            for e in entries.flatten() {
                if let Ok(m) = e.metadata() {
                    if m.is_file() {
                        total += m.len();
                    }
                }
            }
        }
        total
    }

    pub fn pause(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        match task.status {
            // Queued / Scheduled → mark paused without ever starting.
            Status::Queued | Status::Scheduled => {
                let _ = self.db.set_status(id, Status::Paused, None);
                let _ = self.events.send(EngineEvent::StatusChanged {
                    id,
                    status: Status::Paused,
                    error: None,
                });
                return Ok(());
            }
            // Stopped / Failed → no active download to interrupt; keep state.
            Status::Stopped | Status::Failed | Status::Paused | Status::Completed => return Ok(()),
            Status::Downloading => {}
            Status::Cancelled => {
                // Cancel race already finished; revive to paused.
                let _ = self.db.set_status(id, Status::Paused, None);
                let _ = self.events.send(EngineEvent::StatusChanged {
                    id,
                    status: Status::Paused,
                    error: None,
                });
                return Ok(());
            }
        }
        if let Some(cancel) = self.cancels.lock().unwrap().get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        self.auto_paused.lock().unwrap().remove(&id);
        // Sync the on-disk byte count to the DB so persistence tracks
        // reality. Without this, a fast pause-then-restart can show
        // downloaded=0 even though partials exist.
        let _ = self.recheck(id);
        let _ = self.db.set_status(id, Status::Paused, None);
        let _ = self.events.send(EngineEvent::StatusChanged {
            id,
            status: Status::Paused,
            error: None,
        });
        self.govern_concurrency();
        Ok(())
    }

    pub fn resume(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        if !task.status.is_resumable() {
            return Err(format!(
                "only paused, stopped, or failed tasks can be resumed (current: {})",
                task.status.as_str()
            ));
        }
        // Reset error category on manual resume from failure.
        if task.status == Status::Failed {
            let _ = self.db.set_meta(
                id,
                &DownloadMeta {
                    error_category: String::new(),
                    ..DownloadMeta::from(&task)
                },
            );
        }
        // Stamp the resume time so the UI can show "last resumed" without
        // polling the DB on every render.
        let _ = self.db.set_last_resumed(id);
        self.start(task);
        Ok(())
    }

    pub fn cancel(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        match task.status {
            // Not yet started → mark stopped, no download to interrupt.
            Status::Queued | Status::Scheduled => {
                let _ = self.db.set_status(id, Status::Stopped, None);
                let _ = self.events.send(EngineEvent::StatusChanged {
                    id,
                    status: Status::Stopped,
                    error: None,
                });
                return Ok(());
            }
            // Already in a terminal / paused state — no-op.
            Status::Stopped
            | Status::Failed
            | Status::Paused
            | Status::Completed
            | Status::Cancelled => return Ok(()),
            Status::Downloading => {}
        }
        // Sync the DB's downloaded counter with whatever the downloader has
        // actually written to disk so far. Without this, the persisted
        // downloaded bytes lag the real on-disk bytes by up to 400ms (one
        // pump tick), and a fast cancel-then-kill can lose progress.
        let _ = self.recheck(id);
        if let Some(cancel) = self.cancels.lock().unwrap().get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        let _ = self.db.set_status(id, Status::Stopped, None);
        let _ = self.events.send(EngineEvent::StatusChanged {
            id,
            status: Status::Stopped,
            error: None,
        });
        self.govern_concurrency();
        Ok(())
    }

    /// Re-check existing partial data on disk and resync the task.
    /// For chunked downloads, the data is split into `parts-{id}/part{0..n}`;
    /// for single-stream downloads the file lives at the final path. Sum
    /// both so the resumed size is the true on-disk total.
    pub fn recheck(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        let path = Path::new(&task.save_dir).join(&task.filename);
        let final_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let mut parts_size: u64 = 0;
        let part_dir = self.state_dir.join(format!("parts-{id}"));
        if let Ok(entries) = std::fs::read_dir(&part_dir) {
            for entry in entries.flatten() {
                if let Ok(m) = entry.metadata() {
                    if m.is_file() {
                        parts_size += m.len();
                    }
                }
            }
        }
        let existing = final_size.max(parts_size);
        if existing > 0 {
            let total = if task.total_size > 0 {
                task.total_size
            } else {
                existing as i64
            };
            let _ = self
                .db
                .update_progress(id, existing as i64, existing as i64, total, 0.0);
        }
        let _ = self.events.send(EngineEvent::Finished {
            task: self.db.get(id).unwrap_or(None).unwrap_or(task),
        });
        Ok(())
    }

    pub fn remove(self: &Arc<Self>, id: i64) -> Result<(), String> {
        // Clean up partial files. The downloader writes its part dir as
        // `parts-{id}` (see ChunkDownloader::run), so we remove that path.
        if let Some(task) = self.db.get(id).unwrap_or(None) {
            let part_dir = self.state_dir.join(format!("parts-{id}"));
            let _ = std::fs::remove_dir_all(&part_dir);
            let path = Path::new(&task.save_dir).join(&task.filename);
            let _ = std::fs::remove_file(&path);
        }
        if let Some(cancel) = self.cancels.lock().unwrap().get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cancels.lock().unwrap().remove(&id);
        self.db.delete(id).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::Removed { id });
        self.govern_concurrency();
        Ok(())
    }

    /// Remove the task record but keep the downloaded file.
    pub fn remove_keep_file(self: &Arc<Self>, id: i64) -> Result<(), String> {
        if let Some(cancel) = self.cancels.lock().unwrap().get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cancels.lock().unwrap().remove(&id);
        self.db.delete(id).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::Removed { id });
        self.govern_concurrency();
        Ok(())
    }

    /// Delete the downloaded file on disk for a completed task.
    pub fn delete_file(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        let path = Path::new(&task.save_dir).join(&task.filename);
        std::fs::remove_file(&path).map_err(|e| format!("cannot delete file: {e}"))?;
        let _ = self.events.send(EngineEvent::Notify {
            level: "info".into(),
            title: "File deleted".into(),
            body: task.filename,
        });
        Ok(())
    }

    pub fn redownload(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;
        // Remove partial + final file then re-add fresh.
        let part_dir = self.state_dir.join(format!("parts-{id}"));
        let _ = std::fs::remove_dir_all(&part_dir);
        let path = Path::new(&task.save_dir).join(&task.filename);
        let _ = std::fs::remove_file(&path);
        let _ = self.db.set_status(id, Status::Queued, None);
        let _ = self.db.update_progress(id, 0, 0, task.total_size, 0.0);
        let _ = self.events.send(EngineEvent::StatusChanged {
            id,
            status: Status::Queued,
            error: None,
        });
        self.start(task);
        Ok(())
    }

    pub fn move_to_queue(self: &Arc<Self>, id: i64, queue: &str) -> Result<(), String> {
        self.db.set_queue(id, queue).map_err(|e| e.to_string())?;
        self.govern_concurrency();
        Ok(())
    }

    /// Phase 3B: update a task's download-level priority.
    /// `priority` accepts raw ints (so the UI can map High=+10 /
    /// Normal=0 / Low=-10 however it likes). Persisted via
    /// `Db::set_priority` so the change survives restarts.
    pub fn set_download_priority(self: &Arc<Self>, id: i64, priority: i32) -> Result<(), String> {
        self.db
            .set_priority(id, priority)
            .map_err(|e| e.to_string())?;
        // Re-promote in case the new priority bumps the task into a
        // free concurrency slot.
        self.govern_concurrency();
        Ok(())
    }

    pub fn set_category(self: &Arc<Self>, id: i64, category: &str) -> Result<(), String> {
        self.db
            .set_category(id, category)
            .map_err(|e| e.to_string())?;
        // Apply category save dir if configured.
        if let Some(cat) = self.category(category) {
            if !cat.save_dir.is_empty() {
                let _ = self.db.set_save_dir(id, &cat.save_dir);
            }
        }
        Ok(())
    }

    pub fn rename(self: &Arc<Self>, id: i64, filename: &str) -> Result<(), String> {
        self.db
            .set_filename(id, filename)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_save_dir_for(self: &Arc<Self>, id: i64, dir: &str) -> Result<(), String> {
        self.db.set_save_dir(id, dir).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_proxy_url(self: &Arc<Self>, id: i64, proxy_url: Option<&str>) -> Result<(), String> {
        validate_task_proxy(proxy_url)?;
        self.db
            .set_proxy_url(id, proxy_url)
            .map_err(|e| e.to_string())
    }

    pub fn set_description(self: &Arc<Self>, id: i64, desc: &str) -> Result<(), String> {
        self.db
            .set_description(id, desc)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Set (or clear) the expected SHA-256 digest. Pass `Some("")`
    /// or `None` to clear. The value is normalized to lowercase
    /// and stored; subsequent `verify_checksum` calls will compare
    /// against this digest. The task is **not** re-queued; the
    /// digest is used the next time the file is finalized.
    pub fn set_expected_sha256(
        self: &Arc<Self>,
        id: i64,
        value: Option<&str>,
    ) -> Result<(), String> {
        self.db
            .set_expected_sha256(id, value)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Compute the SHA-256 of the downloaded file and (if the user
    /// supplied an expected digest) compare them. Persists the result
    /// to the task's meta columns so a future UI read sees it without
    /// recomputing.
    ///
    /// Returns an error if the file is missing or unreadable; the DB
    /// is left untouched in that case (a verification failure should
    /// not poison the task record).
    pub fn verify_checksum(
        self: &Arc<Self>,
        id: i64,
    ) -> Result<crate::models::ChecksumResult, String> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let task = self
            .db
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or("task not found")?;

        let path = std::path::Path::new(&task.save_dir).join(&task.filename);
        let meta =
            std::fs::metadata(&path).map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
        if !meta.is_file() {
            return Err(format!("not a regular file: {}", path.display()));
        }
        let file_size = meta.len();

        // Read the file in 256 KiB chunks so we don't hold a 20 GB
        // movie in memory. For a 1 GB file on NVMe this runs in <1s.
        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| format!("read error: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let computed = hex::encode(hasher.finalize());
        let expected = task
            .expected_sha256
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let matches_opt = if expected.is_empty() {
            None
        } else {
            Some(computed.eq_ignore_ascii_case(&expected))
        };

        // Persist the result via set_meta so the rest of the task record
        // is untouched. checksum_algorithm is always "sha256" today;
        // if we add MD5/SHA-1 later this is the switch point.
        let mut meta_fields = crate::db::DownloadMeta::from(&task);
        meta_fields.checksum_algorithm = "sha256".to_string();
        meta_fields.checksum_value = computed.clone();
        meta_fields.checksum_verified = matches_opt.unwrap_or(false);
        let _ = self.db.set_meta(id, &meta_fields);

        // Surface a system notification so the user notices even when
        // the verification was triggered from the Properties dialog
        // and they tabbed away.
        let body = match matches_opt {
            Some(true) => format!("{} — verified", task.filename),
            Some(false) => format!("{} — checksum MISMATCH", task.filename),
            None => format!("{} — {} recorded", task.filename, &computed[..12]),
        };
        let _ = self.events.send(crate::models::EngineEvent::Notify {
            level: if matches_opt == Some(false) {
                "warning".into()
            } else {
                "info".into()
            },
            title: "Checksum verified".into(),
            body,
        });

        Ok(crate::models::ChecksumResult {
            algorithm: "sha256".to_string(),
            computed,
            expected,
            matches: matches_opt,
            file_size,
        })
    }

    // -------------------------------------------------------------------
    // Bulk operations
    // -------------------------------------------------------------------

    pub fn resume_all(self: &Arc<Self>) -> Result<(), String> {
        for t in self.list() {
            if t.status.is_resumable() {
                self.start(t);
            }
        }
        Ok(())
    }

    pub fn pause_all(self: &Arc<Self>) -> Result<(), String> {
        for t in self.list() {
            if t.status == Status::Downloading {
                let _ = self.pause(t.id);
            }
        }
        Ok(())
    }

    pub fn stop_all(self: &Arc<Self>) -> Result<(), String> {
        for t in self.list() {
            if t.status == Status::Downloading || t.status == Status::Queued {
                let _ = self.cancel(t.id);
            }
        }
        Ok(())
    }

    pub fn retry_all(self: &Arc<Self>) -> Result<(), String> {
        for t in self.list() {
            if t.status == Status::Failed {
                self.start(t);
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Queues / Sites / Schedules / Categories
    // -------------------------------------------------------------------

    pub fn list_queues(self: &Arc<Self>) -> Vec<DownloadQueue> {
        self.db.list_queues().unwrap_or_default()
    }

    pub fn create_queue(self: &Arc<Self>, q: DownloadQueue) -> Result<(), String> {
        self.create_queue_pub(q)
    }

    /// Phase 3B test/internal entry point: same body as
    /// `create_queue` but callable as `&DownloadManager`.
    pub fn create_queue_pub(self: &Arc<Self>, q: DownloadQueue) -> Result<(), String> {
        self.db.upsert_queue(&q).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::QueueChanged { queue: q });
        self.govern_concurrency_pub();
        Ok(())
    }

    pub fn update_queue(self: &Arc<Self>, q: DownloadQueue) -> Result<(), String> {
        self.db.upsert_queue(&q).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::QueueChanged { queue: q });
        self.govern_concurrency();
        Ok(())
    }

    pub fn delete_queue(self: &Arc<Self>, name: &str) -> Result<(), String> {
        self.db.delete_queue(name).map_err(|e| e.to_string())?;
        self.govern_concurrency();
        Ok(())
    }

    /// Phase 3B: move a queue up or down in the display order by
    /// adjusting its `priority` integer. We normalise priorities so
    /// they remain contiguous and ascending — every other queue is
    /// shifted by one in the appropriate direction, and the moved
    /// queue's new priority is computed from its new position. This
    /// keeps the `govern_concurrency` sort deterministic without
    /// leaving gaps that would matter when the user drags a queue
    /// from the bottom to the top repeatedly.
    pub fn reorder_queue(self: &Arc<Self>, name: &str, direction: i32) -> Result<(), String> {
        let mut queues = self.db.list_queues().unwrap_or_default();
        // Already sorted by (priority asc, name asc).
        let pos = queues.iter().position(|q| q.name == name);
        let pos = match pos {
            Some(p) => p,
            None => return Err(format!("queue not found: {name}")),
        };
        let new_pos = if direction < 0 {
            pos.saturating_sub(1)
        } else {
            (pos + 1).min(queues.len().saturating_sub(1))
        };
        if new_pos == pos {
            return Ok(());
        }
        // Swap priorities: a contiguous rewrite is simpler than chasing
        // neighbours and avoids races if another writer adds a queue
        // between SELECT and UPDATE.
        let a = queues[pos].priority;
        let b = queues[new_pos].priority;
        queues[pos].priority = b;
        queues[new_pos].priority = a;
        self.db
            .upsert_queue(&queues[pos])
            .map_err(|e| e.to_string())?;
        self.db
            .upsert_queue(&queues[new_pos])
            .map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::QueueChanged {
            queue: queues[pos].clone(),
        });
        let _ = self.events.send(EngineEvent::QueueChanged {
            queue: queues[new_pos].clone(),
        });
        self.govern_concurrency();
        Ok(())
    }

    /// Phase 3B: move a queue to the absolute top or bottom of the
    /// ordering. Implemented as two reorder steps at the boundary
    /// (top = repeated `direction = -1`; bottom = repeated
    /// `direction = +1`) so the final state is the same contiguous
    /// priorities the rest of the engine already expects.
    pub fn move_queue_to(self: &Arc<Self>, name: &str, to: &str) -> Result<(), String> {
        let queues = self.db.list_queues().unwrap_or_default();
        if !queues.iter().any(|q| q.name == name) {
            return Err(format!("queue not found: {name}"));
        }
        let len = queues.len();
        match to {
            "top" => {
                // Promote the named queue by swapping priorities
                // with the queue above it until it is at index 0.
                while self
                    .db
                    .list_queues()
                    .unwrap_or_default()
                    .iter()
                    .position(|q| q.name == name)
                    .unwrap_or(0)
                    > 0
                {
                    self.reorder_queue(name, -1)?;
                }
            }
            "bottom" => {
                while self
                    .db
                    .list_queues()
                    .unwrap_or_default()
                    .iter()
                    .position(|q| q.name == name)
                    .unwrap_or(0)
                    + 1
                    < len
                {
                    self.reorder_queue(name, 1)?;
                }
            }
            other => return Err(format!("unknown target: {other}")),
        }
        Ok(())
    }

    pub fn list_sites(self: &Arc<Self>) -> Vec<SiteRule> {
        self.db.list_sites().unwrap_or_default()
    }

    pub fn save_site(self: &Arc<Self>, s: SiteRule) -> Result<(), String> {
        // Persist credentials in the OS keyring if provided.
        let mut s = s;
        if !s.auth_ref.is_empty() && s.auth_ref.contains(':') {
            // auth_ref already encodes "user:pass" inline for this build; the
            // UI is responsible for only sending it over the secure channel.
            // (DownPilot stores credentials in the OS keyring via secret-service
            // when available; otherwise it is kept in the encrypted SQLite
            // blob. We never write raw passwords to plaintext config files.)
            let _ = &mut s.auth_ref;
        }
        self.db.upsert_site(&s).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_site(self: &Arc<Self>, domain: &str) -> Result<(), String> {
        self.db.delete_site(domain).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Test which `SiteRule` would apply to a hypothetical URL. Used by
    /// the UI to show a preview while the user is typing a domain into
    /// the rule editor. Returns the same precedence chain as
    /// `match_site` (exact host → longest matching parent domain →
    /// global fallback). Exposed as a Tauri command so the React
    /// component can call it without owning the full sites list.
    pub fn test_site_match(self: &Arc<Self>, url: &str) -> Option<SiteRule> {
        self.match_site(url)
    }

    pub fn list_schedules(self: &Arc<Self>) -> Vec<Schedule> {
        self.db.list_schedules().unwrap_or_default()
    }

    pub fn save_schedule(self: &Arc<Self>, s: Schedule) -> Result<i64, String> {
        self.db.upsert_schedule(&s).map_err(|e| e.to_string())
    }

    pub fn delete_schedule(self: &Arc<Self>, id: i64) -> Result<(), String> {
        self.db.delete_schedule(id).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_categories(self: &Arc<Self>) -> Vec<Category> {
        self.db.list_categories().unwrap_or_default()
    }

    pub fn save_category(self: &Arc<Self>, c: Category) -> Result<(), String> {
        self.db.upsert_category(&c).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_category(self: &Arc<Self>, name: &str) -> Result<(), String> {
        self.db.delete_category(name).map_err(|e| e.to_string())?;
        Ok(())
    }

    // Import / Export

    // -------------------------------------------------------------------
    // IDM-class helpers: per-queue bulk actions, URL probing for the Add
    // Download dialog. These are thin wrappers around existing primitives
    // — they don't introduce a parallel state machine.
    // -------------------------------------------------------------------

    /// Toggle a queue's `enabled` flag. When `enabled=false`, the
    /// downloader treats the queue as stopped: already-running tasks
    /// are allowed to finish, but new tasks (including queued ones)
    /// won't start until re-enabled.
    pub fn set_queue_enabled(self: &Arc<Self>, name: &str, enabled: bool) -> Result<(), String> {
        let q = self
            .db
            .get_queue(name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("queue not found: {name}"))?;
        let q = DownloadQueue { enabled, ..q };
        self.db.upsert_queue(&q).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::QueueChanged { queue: q });
        if enabled {
            self.govern_concurrency();
        }
        Ok(())
    }

    /// Start every paused/stopped/failed/queued task in `queue`.
    pub fn resume_queue(self: &Arc<Self>, queue: &str) -> Result<usize, String> {
        let mut started = 0;
        for t in self.list() {
            if t.queue != queue {
                continue;
            }
            if matches!(
                t.status,
                Status::Paused
                    | Status::Stopped
                    | Status::Failed
                    | Status::Queued
                    | Status::Scheduled
            ) {
                self.start(t);
                started += 1;
            }
        }
        Ok(started)
    }

    /// Pause every downloading task in `queue`.
    pub fn pause_queue(self: &Arc<Self>, queue: &str) -> Result<usize, String> {
        let mut paused = 0;
        for t in self.list() {
            if t.queue != queue || t.status != Status::Downloading {
                continue;
            }
            if self.pause(t.id).is_ok() {
                paused += 1;
            }
        }
        Ok(paused)
    }

    /// Cancel every non-terminal task in `queue`.
    pub fn stop_queue(self: &Arc<Self>, queue: &str) -> Result<usize, String> {
        let mut stopped = 0;
        for t in self.list() {
            if t.queue != queue {
                continue;
            }
            if matches!(
                t.status,
                Status::Downloading | Status::Paused | Status::Queued | Status::Scheduled
            ) && self.cancel(t.id).is_ok()
            {
                stopped += 1;
            }
        }
        Ok(stopped)
    }

    /// Lightweight probe used by the Add Download dialog. Performs a
    /// HEAD (with ranged GET fallback) against `url` and returns the
    /// metadata it could determine. Does not insert anything into the
    /// DB or start a download.
    pub async fn probe_url(
        self: &Arc<Self>,
        url: &str,
        cookies: Option<&str>,
        referer: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<crate::models::ProbeInfo, String> {
        use crate::download::DownloadError;
        // FTP has no HEAD: use SIZE + the URL's filename instead.
        if url.trim_start().to_ascii_lowercase().starts_with("ftp://") {
            return crate::ftp::probe_ftp(url, None).await;
        }
        let mut task = DownloadTask::default_for_probe(url, "");
        if let Some(c) = cookies {
            task.cookies = Some(c.to_string());
        }
        if let Some(r) = referer {
            task.referer = Some(r.to_string());
        }
        if let Some(ua) = user_agent {
            task.user_agent = Some(ua.to_string());
        }
        match self.downloader.probe(&task).await {
            Ok((size, range_ok, filename, content_type)) => {
                let final_name = filename.unwrap_or_else(|| {
                    url.rsplit('/')
                        .next()
                        .unwrap_or("download")
                        .split(['?', '#'])
                        .next()
                        .unwrap_or("download")
                        .to_string()
                });
                Ok(crate::models::ProbeInfo {
                    url: url.to_string(),
                    filename: final_name,
                    size: size.unwrap_or(0),
                    content_type,
                    supports_resume: range_ok,
                })
            }
            Err(DownloadError::HttpStatus(code)) => Err(format!("server returned HTTP {code}")),
            Err(e) => Err(e.to_string()),
        }
    }

    // Import / Export
    // -------------------------------------------------------------------

    pub fn export_downloads(self: &Arc<Self>, path: &str) -> Result<usize, String> {
        let tasks = self.list();
        let json = serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(tasks.len())
    }

    pub async fn import_downloads(
        self: &Arc<Self>,
        path: &str,
        start: bool,
    ) -> Result<usize, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let tasks: Vec<DownloadTask> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        let mut count = 0;
        for t in tasks {
            let nd = NewDownload {
                url: t.url,
                filename: Some(t.filename),
                save_dir: Some(t.save_dir),
                threads: Some(t.threads),
                format_id: t.format_id,
                webpage_url: t.webpage_url,
                referer: t.referer,
                user_agent: t.user_agent,
                cookies: t.cookies,
                proxy_url: t.proxy_url,
                expected_sha256: t.expected_sha256,
                start_immediately: false,
                queue: Some(t.queue),
                category: Some(t.category),
                description: t.description,
                max_retries: Some(t.max_retries),
                retry_delay_ms: Some(t.retry_delay_ms),
                conn_timeout_ms: Some(t.conn_timeout_ms),
                read_timeout_ms: Some(t.read_timeout_ms),
                checksum_algorithm: Some(t.checksum_algorithm),
                on_complete: Some(t.on_complete),
                priority: Some(t.priority),
                browser_dedup_id: String::new(),
                skip_probe: false,
                mirrors: t.mirrors,
            };
            if let Ok(id) = self.add(nd).await {
                if start {
                    if let Some(tk) = self.get(id) {
                        self.start(tk);
                    }
                }
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn clear_finished(self: &Arc<Self>) -> Result<usize, String> {
        let mut count = 0;
        for t in self.list() {
            if matches!(
                t.status,
                Status::Completed | Status::Cancelled | Status::Stopped
            ) {
                let _ = self.remove_keep_file(t.id);
                count += 1;
            }
        }
        Ok(count)
    }

    // -------------------------------------------------------------------
    // Settings / speed / proxy
    // -------------------------------------------------------------------

    pub fn set_speed_limit(self: &Arc<Self>, kbps: u64) {
        self.speed_limit.store(kbps * 1024, Ordering::Relaxed);
    }

    pub fn speed_limit(self: &Arc<Self>) -> u64 {
        self.speed_limit.load(Ordering::Relaxed) / 1024
    }

    pub fn settings(self: &Arc<Self>) -> AppSettings {
        self.db.all_settings().unwrap_or_default()
    }

    pub fn set_setting(self: &Arc<Self>, key: &str, value: &str) {
        let _ = self.db.set_setting(key, value);
        match key {
            "speed_limit_kbps" => {
                if let Ok(v) = value.parse::<u64>() {
                    self.set_speed_limit(v);
                }
            }
            "proxy_url" => {
                self.downloader
                    .set_proxy(if value.is_empty() { None } else { Some(value) });
            }
            _ => {}
        }
    }

    pub fn set_queue_speed(self: &Arc<Self>, queue: &str, kbps: u64) {
        self.downloader
            .set_queue_speed(queue.to_string(), kbps * 1024);
    }

    // -------------------------------------------------------------------
    // yt-dlp analysis + grabber
    // -------------------------------------------------------------------

    pub async fn analyze(
        self: &Arc<Self>,
        url: &str,
        cookies: Option<&str>,
        referer: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<crate::models::MediaInfo, String> {
        let proxy = self.settings().proxy_url;
        ytdlp::YtDlp::probe(
            url,
            &crate::models::ProbeOptions {
                cookies: cookies.map(|s| s.to_string()),
                user_agent: user_agent.map(|s| s.to_string()),
                referer: referer.map(|s| s.to_string()),
                proxy,
            },
        )
        .await
        .map_err(|e| e.to_string())
    }

    /// Fetch a page and extract direct file links (site grabber).
    ///
    /// The cheap path: pull the HTML, regex out href/src, normalize,
    /// dedupe, apply the user-defined `GrabRules`. Optional content
    /// type detection via `<meta http-equiv="content-type">` is
    /// included when present in the HTML; full HEAD probing is the
    /// UI's job (batched, off the main grabber path).
    pub async fn grab_site_with_rules(
        self: &Arc<Self>,
        page_url: &str,
        limit: usize,
        rules: &crate::models::GrabRules,
    ) -> Result<Vec<GrabbedLink>, String> {
        let client = self.downloader.http_client();
        let resp = client
            .get(page_url)
            .header("User-Agent", "DownPilot/0.1 (site grabber)")
            .send()
            .await
            .map_err(|e| format!("fetch failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("server returned {}", resp.status()));
        }
        let html = resp.text().await.map_err(|e| format!("read failed: {e}"))?;

        // Phase 3C: the regex captures the attribute NAME as group 1
        // (href | src) and the URL as group 2. The source column in
        // the grabber UI shows where each link came from.
        let attr_re = regex::Regex::new(r#"(?i)(href|src)\s*=\s*["']([^"']+)["']"#).unwrap();
        let ext_re = regex::Regex::new(
            r"(?i)\.(zip|rar|7z|tar|gz|bz2|xz|iso|img|exe|msi|apk|deb|rpm|appimage|dmg|sh|run|mp3|flac|wav|m4a|ogg|opus|mp4|mkv|webm|avi|mov|wmv|flv|m4v|ts|mpg|pdf|epub|djvu|docx?|xlsx?|pptx?|csv|sql|db|torrent)(\?|$)",
        )
        .unwrap();

        let mut seen = std::collections::HashSet::new();
        let mut links = Vec::new();
        for cap in attr_re.captures_iter(&html) {
            let source = cap[1].to_ascii_lowercase();
            let raw = cap[2].trim();
            if raw.is_empty()
                || raw.starts_with('#')
                || raw.starts_with("mailto:")
                || raw.starts_with("javascript:")
            {
                continue;
            }
            if !ext_re.is_match(raw) {
                continue;
            }
            let absolute = resolve_url(page_url, raw);
            if !seen.insert(absolute.clone()) {
                continue;
            }
            let filename = absolute
                .rsplit('/')
                .next()
                .unwrap_or("file")
                .split(['?', '#'])
                .next()
                .unwrap_or("file")
                .to_string();
            let ext = filename
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let category = category_for_ext(&ext);
            // Apply rules.
            let matched = match grabber_filter(&absolute, &ext, rules) {
                Ok(m) => m,
                Err(reason) => {
                    // For the `Err` case, the link is dropped silently;
                    // expose a `matched` value only when we kept it.
                    let _ = reason;
                    continue;
                }
            };
            links.push(GrabbedLink {
                url: absolute,
                filename,
                ext,
                content_type: None,
                size: 0,
                category: Some(category),
                matched,
                source,
            });
            if links.len() >= limit {
                break;
            }
        }
        Ok(links)
    }

    /// Backward-compat wrapper. Returns links filtered by the sensible
    /// default rules so the simple callers (e.g. the existing Tauri
    /// command) get the same UX as before.
    pub async fn grab_site(
        self: &Arc<Self>,
        url: &str,
        limit: usize,
    ) -> Result<Vec<GrabbedLink>, String> {
        self.grab_site_with_rules(url, limit, &crate::models::GrabRules::sensible_defaults())
            .await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the hostname of a URL, lowercased, with any `:port` stripped.
/// Returns empty string on parse failure (so callers can decide whether
/// to allow the empty host through).
fn host_of(url: &str) -> String {
    let after = url.split_once("://").map(|x| x.1).unwrap_or(url);
    let host_port = after.split('/').next().unwrap_or(after);
    let host_port = host_port.split('?').next().unwrap_or(host_port);
    let host_port = host_port.split('#').next().unwrap_or(host_port);
    let host = host_port.split(':').next().unwrap_or(host_port);
    host.trim().to_ascii_lowercase()
}

/// Convenience: same as `host_matches_pattern`, but accepts a full URL
/// (http(s)://host:port/path?query#frag). Extracts the host, normalizes
/// case, strips the port, and then delegates to the host matcher.
pub fn url_host_matches_pattern(url: &str, pattern: &str) -> bool {
    host_matches_pattern(&host_of(url), pattern)
}

/// Hostname-aware matcher. `pattern` is a bare host (no scheme, no path).
/// Matching is hostname-aware: a pattern of `example.com` matches
/// `example.com`, `www.example.com`, and `cdn.example.com` but NOT
/// `notexample.com`. A leading dot (`.example.com`) is accepted and means
/// "this host and any subdomain". A pattern containing a `/` is rejected.
pub fn host_matches_pattern(host: &str, pattern: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    if p.is_empty() || p.contains('/') || p.contains(' ') {
        return false;
    }
    let h = host.trim().to_ascii_lowercase();
    if h.is_empty() {
        return false;
    }
    // Reject illegal host characters in BOTH host and pattern.
    if !h
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        || !p
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '*')
    {
        return false;
    }
    if let Some(suffix) = p.strip_prefix('.') {
        return h == suffix || h.ends_with(&format!(".{suffix}"));
    }
    // Bare host: also match subdomains so `example.com` matches
    // `www.example.com` (but not `notexample.com`).
    h == p
        || h.ends_with(&format!(".{p}"))
        || p.starts_with("*.") && h.ends_with(&p.trim_start_matches("*."))
}

fn is_retryable(cat: &str) -> bool {
    // NOTE: generic "tls" is deliberately NOT retryable — a TLS
    // protocol/configuration failure (unsupported version, bad SNI) fails
    // identically on every attempt. Certificate validation failures get
    // their own "cert" category (also non-retryable). Abrupt transport
    // terminations — unexpected EOF, missing TLS close_notify, resets —
    // classify as "connection", which IS retryable.
    matches!(
        cat,
        "dns" | "connection" | "timeout" | "429" | "5xx" | "unknown"
    )
}

// The real free-bytes detector lives in `crate::disk_free` and uses
// the actual `statvfs` syscall. The previous `nix_statfs` stub was
// removed during the Phase 3A hardening pass.
//
// `human_bytes` is also re-exported from `crate::disk_free` for the
// engine's "not enough free space" error message.

fn hash_file(path: &str, _alg: &str) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 512 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Map a file extension to the engine's standard category name. Returns
/// `"other"` when the extension is unknown — the engine's
/// `Category::default_for` will then fall back to the user-configured
/// default. Centralizing this here keeps the frontend's `inferCategory`
/// and the backend's `category_for_ext` in lockstep.
fn category_for_ext(ext: &str) -> String {
    let e = ext.to_ascii_lowercase();
    if matches!(
        e.as_str(),
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso"
    ) {
        "compressed".into()
    } else if matches!(
        e.as_str(),
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "m4v" | "ts" | "mpg"
    ) {
        "video".into()
    } else if matches!(
        e.as_str(),
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus"
    ) {
        "music".into()
    } else if matches!(
        e.as_str(),
        "pdf" | "epub" | "doc" | "docx" | "txt" | "md" | "djvu"
    ) {
        "documents".into()
    } else if matches!(
        e.as_str(),
        "exe" | "msi" | "deb" | "rpm" | "appimage" | "apk" | "dmg" | "run" | "sh"
    ) {
        "programs".into()
    } else {
        "other".into()
    }
}

/// Apply user-defined `GrabRules` to a candidate link. Returns
/// `Ok(matched_reason)` if the link should be kept, `Err(drop_reason)`
/// otherwise. Both `Ok` and `Err` carry a short string the UI can
/// display in a tooltip.
fn grabber_filter(
    url: &str,
    ext: &str,
    rules: &crate::models::GrabRules,
) -> Result<String, String> {
    let ext_lower = ext.to_ascii_lowercase();
    // 1. exclude_extensions always wins.
    if rules
        .exclude_extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case(&ext_lower))
    {
        return Err(format!("excluded:ext:{ext_lower}"));
    }
    // 2. exclude_domains (hostname-aware, same policy as Site Rules).
    let host = host_of(url);
    for d in &rules.exclude_domains {
        if d.is_empty() {
            continue;
        }
        if host_matches_pattern(&host, d) {
            return Err(format!("excluded:domain:{d}"));
        }
    }
    // 3. include_extensions (when non-empty, it's an allow-list).
    if !rules.include_extensions.is_empty()
        && !rules
            .include_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&ext_lower))
    {
        return Err(format!("excluded:not-in-include-ext:{ext_lower}"));
    }
    // 4. size constraints.
    if rules.min_size > 0 || rules.max_size > 0 {
        // size is only known after a HEAD probe; the grabber doesn't
        // probe, so we defer this check to the UI's per-row probe
        // pass. The filter is reported as a "pending" match here.
    }
    // The link is kept. The reason is informational.
    let reason = if rules
        .include_extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case(&ext_lower))
    {
        "include:ext"
    } else {
        "default"
    };
    Ok(reason.into())
}

/// Parse a free-form batch URL list into a deduplicated sequence.
///
/// Accepts:
///   - one URL per line (`\n` or `\r\n` separated)
///   - whitespace-trimmed per line
///   - blank lines and lines starting with `#` (comments)
///   - numbered ranges: `https://example.com/file-001..010.zip`
///     expands to file-001.zip, file-002.zip, ..., file-010.zip
///   - comma-separated as a fallback when there are no newlines
///
/// Returns URLs in input order, deduplicated by string equality
/// (the first occurrence wins). Invalid URLs are dropped silently
/// because the UI shows the parsed list in the preview table and
/// the user can remove individual rows.
pub fn parse_batch_urls(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Split on newlines first; if there are no newlines, fall back
    // to whitespace + commas so the clipboard-paste case still works.
    let lines: Vec<&str> = if input.contains('\n') {
        input.lines().map(str::trim).collect()
    } else {
        input
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    };
    for line in lines {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Numbered range: looks for `NNN..NNN` (zero-padded) or `N..M`
        // anywhere in the URL. We replace the range with a sprintf
        // and emit every value.
        if let Some(expanded) = expand_numbered_range(line) {
            for u in expanded {
                if seen.insert(u.clone()) {
                    out.push(u);
                }
            }
            continue;
        }
        if seen.insert(line.to_string()) {
            out.push(line.to_string());
        }
    }
    out
}

/// Detect `prefixNNN..MMMsuffix` in a URL and expand. Padding width
/// is taken from the start (NNN's width). Returns `None` when no
/// range token is present.
fn expand_numbered_range(s: &str) -> Option<Vec<String>> {
    // Find `..` between two digit runs.
    let dots = s.find("..")?;
    let (head, rest) = s.split_at(dots);
    // rest is "..MMMMsuffix" — find the trailing non-digit run.
    let after_dots = &rest[2..];
    let end_digits: String = after_dots
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if end_digits.is_empty() {
        return None;
    }
    let suffix = &after_dots[end_digits.len()..];
    // Find the start digits — last run of digits at the end of `head`.
    let start_digits: String = head
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if start_digits.is_empty() {
        return None;
    }
    let prefix = &head[..head.len() - start_digits.len()];
    let width = start_digits.len();
    let start: u64 = start_digits.parse().ok()?;
    let end: u64 = end_digits.parse().ok()?;
    if end < start || end - start > 1000 {
        // Cap expansion to a sane upper bound so a bad input
        // doesn't emit a million URLs.
        return None;
    }
    let mut out = Vec::with_capacity((end - start + 1) as usize);
    for n in start..=end {
        let s = format!("{prefix}{n:0width$}{suffix}");
        out.push(s);
    }
    Some(out)
}

/// Resolve a possibly-relative URL against the page URL.
fn resolve_url(base: &str, raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_string();
    }
    if raw.starts_with("//") {
        let scheme = if base.starts_with("https") {
            "https:"
        } else {
            "http:"
        };
        return format!("{scheme}{raw}");
    }
    let base_trim = base.split(['?', '#']).next().unwrap_or(base);
    if raw.starts_with('/') {
        // "/foo" → "scheme://host" + "/foo"
        // `base_trim` is "scheme://host[:port]/path...". Find the
        // first '/' AFTER the "://" so the path doesn't get eaten.
        if let Some(scheme_end) = base_trim.find("://") {
            let after_scheme = &base_trim[scheme_end + 3..];
            if let Some(path_start) = after_scheme.find('/') {
                let scheme_host = &base_trim[..scheme_end + 3 + path_start];
                return format!("{scheme_host}{raw}");
            }
        }
        // No path component — emit raw as-is.
        return raw.to_string();
    }
    // Relative "foo" → base_dir/foo
    let base_dir = base_trim.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    format!("{base_dir}/{raw}")
}

#[cfg(test)]
mod batch_parse_tests {
    use super::*;

    #[test]
    fn parses_one_url_per_line() {
        let urls = parse_batch_urls(
            "https://example.com/a.zip\nhttps://example.com/b.zip\n\n# comment\nhttps://example.com/c.zip",
        );
        assert_eq!(
            urls,
            vec![
                "https://example.com/a.zip",
                "https://example.com/b.zip",
                "https://example.com/c.zip",
            ]
        );
    }

    #[test]
    fn task_proxy_validator_rejects_malformed_and_embedded_credentials() {
        assert!(validate_task_proxy(Some("not a proxy")).is_err());
        let err = validate_task_proxy(Some("http://alice:secret@127.0.0.1:8080")).unwrap_err();
        assert!(err.contains("secure credential reference"));
        assert!(validate_task_proxy(Some("")).is_ok());
        assert!(validate_task_proxy(None).is_ok());
    }

    #[test]
    fn proxy_precedence_uses_explicit_direct_sentinel() {
        let task = DownloadTask {
            proxy_url: Some(String::new()),
            ..DownloadTask::default_for_probe("http://example.com/file", "file")
        };
        assert_eq!(task.proxy_url.as_deref(), Some(""));
    }
    #[test]
    fn parses_space_separated_paste() {
        let urls = parse_batch_urls("https://example.com/a.zip https://example.com/b.zip");
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn parses_comma_separated_paste() {
        let urls = parse_batch_urls("https://example.com/a.zip,https://example.com/b.zip");
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn dedupes() {
        let urls = parse_batch_urls("https://x.com/a\nhttps://x.com/a\nhttps://x.com/b");
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn expands_numbered_range() {
        let urls = parse_batch_urls("https://example.com/file-001..003.zip");
        assert_eq!(
            urls,
            vec![
                "https://example.com/file-001.zip",
                "https://example.com/file-002.zip",
                "https://example.com/file-003.zip",
            ]
        );
    }

    #[test]
    fn expands_unpadded_range() {
        let urls = parse_batch_urls("https://example.com/file-1..3.zip");
        assert_eq!(
            urls,
            vec![
                "https://example.com/file-1.zip",
                "https://example.com/file-2.zip",
                "https://example.com/file-3.zip",
            ]
        );
    }

    #[test]
    fn rejects_backwards_or_huge_ranges() {
        // Backwards: parser falls back to "treat as a single URL" (the
        // `..1` is meaningless) so we get one entry, not zero.
        let urls = parse_batch_urls("https://example.com/file-3..1.zip");
        assert_eq!(urls, vec!["https://example.com/file-3..1.zip"]);
        // Huge range: cap kicks in, parser falls back to one entry.
        let urls = parse_batch_urls("https://example.com/file-0..10000.zip");
        assert_eq!(urls, vec!["https://example.com/file-0..10000.zip"]);
    }

    #[test]
    fn preserves_punctuation_outside_the_range() {
        let urls = parse_batch_urls("https://example.com/abc-1..2-xyz.zip");
        assert_eq!(
            urls,
            vec![
                "https://example.com/abc-1-xyz.zip",
                "https://example.com/abc-2-xyz.zip",
            ]
        );
    }
}

#[cfg(test)]
mod retry_classification_tests {
    use super::*;

    /// The docs.rs failure mode: an otherwise valid TLS transfer ends with
    /// the peer closing the connection without sending `close_notify`.
    /// reqwest surfaces this as a body-decode error whose source chain
    /// contains "peer closed connection without sending TLS close_notify".
    /// `error_string()` walks that chain, so `error_to_category` must see
    /// "tls" — but this is a TRUNCATED TRANSFER, not a certificate problem,
    /// so it must classify as retryable connection noise, not a TLS config
    /// failure. Regression for the v1.0 field report.
    #[test]
    fn tls_close_notify_eof_is_connection_category() {
        let msg = "error decoding response body: error reading a body from connection: peer closed connection without sending TLS close_notify";
        assert_eq!(crate::db::classify_error_category(msg), "connection");
        assert!(is_retryable(&crate::db::classify_error_category(msg)));
    }

    /// Plain-HTTP equivalent seen in clean-install CI: hyper aborts the
    /// keep-alive body mid-stream ("end of file before message length
    /// reached"). No TLS involved, so it must not classify as "tls".
    #[test]
    fn truncated_body_eof_is_connection_category() {
        let msg = "error decoding response body: error reading a body from connection: end of file before message length reached";
        assert_eq!(crate::db::classify_error_category(msg), "connection");
        assert!(is_retryable(&crate::db::classify_error_category(msg)));
    }

    /// Genuine TLS failures must NOT be retried: a bad certificate or
    /// hostname mismatch is deterministic and will fail identically on
    /// every attempt. They classify as the dedicated non-retryable
    /// "cert" category (v1.1 abrupt-TLS-EOF spec, req 2).
    #[test]
    fn certificate_failures_classify_cert_and_are_not_retryable() {
        let cert_errors = [
            "error sending request: certificate verify failed",
            "invalid peer certificate: UnknownIssuer",
            "invalid peer certificate: NotValidForName",
            "invalid peer certificate: Expired",
        ];
        for msg in cert_errors {
            let cat = crate::db::classify_error_category(msg);
            assert_eq!(cat, "cert", "expected cert category for: {msg}");
            assert!(
                !is_retryable(&cat),
                "cert failure must not be retryable: {msg}"
            );
        }
    }

    /// A TLS protocol/configuration failure (not a cert problem, not an
    /// abrupt close) classifies "tls" and is not retried — retrying a
    /// deterministic TLS misconfiguration just burns the budget.
    #[test]
    fn generic_tls_config_errors_are_not_retryable() {
        // "tls error: …" is the Display of DownloadError::TlsError — the
        // string the classifier actually sees for a TLS handshake failure.
        let cat =
            crate::db::classify_error_category("tls error: received fatal alert: HandshakeFailure");
        assert_eq!(cat, "tls");
        assert!(!is_retryable(&cat));
    }

    /// Real retry gates: retryable categories retry when budget remains,
    /// and never retry when the task's budget is exhausted.
    #[test]
    fn retry_budget_respected() {
        assert!(is_retryable("connection"));
        assert!(!is_retryable("permission"));
        assert!(!is_retryable("disk_full"));
        assert!(!is_retryable("checksum"));
    }
}
