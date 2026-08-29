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

use crate::db::{DownloadMeta, Db};
use crate::download::ChunkDownloader;
use crate::models::{
    AppSettings, Category, DownloadQueue, DownloadTask, EngineEvent, ErrorClass, GrabbedLink,
    NewDownload, Schedule, SiteRule, Status,
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
    db: Db,
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
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
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
                    parts.join(": ")
                }
                other => other.to_string(),
            },
            TaskError::YtDlp(y) => y.to_string(),
            TaskError::Cancelled => "cancelled".to_string(),
        }
    }

    pub fn new(db_path: &Path, state_dir: PathBuf) -> (Arc<Self>, mpsc::UnboundedReceiver<EngineEvent>) {
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
        });
        (m, rx)
    }

    /// Pump loop: drains internal events, computes speeds, persists progress
    /// to SQLite, forwards to the UI sink, and enforces the schedule window.
    pub fn pump_loop(
        self: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<EngineEvent>,
        sink: mpsc::UnboundedSender<EngineEvent>,
    ) {
        tokio::spawn(async move {
            let mut speeds: HashMap<i64, (Instant, i64)> = HashMap::new();
            let mut tick = tokio::time::interval(Duration::from_millis(400));
            loop {
                tick.tick().await;
                while let Ok(ev) = rx.try_recv() {
                    let fwd = match &ev {
                        EngineEvent::Progress { id, downloaded, total, .. } => {
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
                            let speed = match speeds.get(&id) {
                                Some((last_t, last_b)) => {
                                    let dt = now_t.duration_since(*last_t).as_secs_f64();
                                    if dt >= 0.4 {
                                        let s = ((*downloaded - *last_b) as f64 / dt).max(0.0);
                                        speeds.insert(*id, (now_t, *downloaded));
                                        s
                                    } else {
                                        speeds.get(&id).map(|_| 0.0).unwrap_or(0.0)
                                    }
                                }
                                None => {
                                    speeds.insert(*id, (now_t, *downloaded));
                                    0.0
                                }
                            };
                            let _ = self.db.update_progress(*id, *downloaded, *downloaded, *total, speed);
                            EngineEvent::Progress { id: *id, downloaded: *downloaded, total: *total, speed_bps: speed }
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
                        self.downloader.set_queue_speed(q.name.clone(), s.bandwidth_kbps as u64);
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
            } else if run && self.auto_paused.lock().unwrap().remove(&t.id) && t.status == Status::Paused {
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
        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.id.cmp(&b.id))
        });

        for s in &candidates {
            let active = self.schedule_active_now(s);
            if !active {
                continue;
            }
            // Parse per-queue overrides; ignore malformed entries.
            let parsed: HashMap<String, i64> = match serde_json::from_str(&s.overrides_json) {
                Ok(v) => v,
                Err(_) => HashMap::new(),
            };
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
                    self.downloader
                        .set_queue_speed(qname.clone(), *kbps * 1024);
                    active_now.insert(qname.clone(), *kbps);
                }
                None => {
                    // Deactivate: restore the captured baseline. If
                    // the user changed the configured speed while the
                    // schedule was running, the schedule "leaves"
                    // without overwriting — the user's new setting
                    // sticks.
                    let prev = baseline
                        .remove(&qname)
                        .unwrap_or(configured_kbps);
                    self.downloader
                        .set_queue_speed(qname.clone(), prev * 1024);
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

    /// Enforce the global + per-queue maximum simultaneous downloads.
    fn govern_concurrency(self: &Arc<Self>) {
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
        let mut queued: Vec<&DownloadTask> = tasks.iter().filter(|t| t.status == Status::Queued).collect();
        // Order by queue priority then created_at.
        queued.sort_by_key(|t| {
            let prio = queues.iter().find(|q| q.name == t.queue).map(|q| q.priority).unwrap_or(0);
            (prio, t.created_at)
        });

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
        // Browser dedup: if the same chrome.downloads.onCreated event fires
        // twice (Chrome retry, devtools refresh, etc.) we don't want two
        // DownPilot tasks. Return the existing id instead of inserting.
        if !nd.browser_dedup_id.is_empty() {
            if let Ok(Some(existing)) = self.db.find_by_browser_dedup(&nd.browser_dedup_id) {
                return Ok(existing);
            }
        }
        let save_dir = match nd.save_dir.clone() {
            Some(d) => d,
            None => self.resolve_save_dir(&nd),
        };
        if !Path::new(&save_dir).exists() {
            std::fs::create_dir_all(&save_dir).map_err(|e| format!("cannot create save dir: {e}"))?;
        }
        // Disk space pre-check.
        if let Some(needed) = self.estimate_size(&nd) {
            if !self.has_disk_space(&save_dir, needed) {
                return Err(format!(
                    "Not enough free disk space in {save_dir} (need at least {}).",
                    human_bytes(needed)
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
                if !site.queue.is_empty() {
                    let _ = self.db.set_queue(id, &site.queue);
                }
                if !site.save_dir.is_empty() {
                    let _ = self.db.set_save_dir(id, &site.save_dir);
                }
                if !site.category.is_empty() {
                    let _ = self.db.set_category(id, &site.category);
                }
                if site.bandwidth_kbps > 0 && !site.queue.is_empty() {
                    self.downloader.set_queue_speed(site.queue.clone(), site.bandwidth_kbps as u64);
                }
                if !site.user_agent.is_empty() {
                    let _ = self.db.set_user_agent(id, &site.user_agent);
                }
                // `auth_ref` and `proxy_url` are loaded by the
                // downloader's request builder via the queue + settings
                // at byte-flow time. Storing them on the task is
                // reserved for a future per-request auth override.
            }
            let _ = self.events.send(EngineEvent::Added { task: task.clone() });
            if nd.start_immediately {
                // Respect concurrency immediately.
                let _ = self.govern_concurrency();
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
            let headers = DownloadTask {
                user_agent: task.user_agent.clone(),
                referer: task.referer.clone(),
                cookies: task.cookies.clone(),
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
                // skip starting the download.
                let err_str = e.to_string();
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
        // Site rule takes precedence.
        if let Some(url) = nd.webpage_url.as_ref().or(Some(&nd.url)) {
            if let Some(site) = self.match_site(url) {
                if !site.save_dir.is_empty() {
                    return site.save_dir;
                }
            }
        }
        // Category rule.
        if let Some(cat_name) = nd.category.clone() {
            if let Some(c) = self.category(&cat_name) {
                if !c.save_dir.is_empty() {
                    return c.save_dir;
                }
            }
        }
        // Generic category guess from filename.
        let fname = nd.filename.clone().unwrap_or_else(|| crate::db::guess_filename(&nd.url));
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
        match std::fs::metadata(dir) {
            Ok(m) => {
                if let Ok(stat) = nix_statfs(dir) {
                    stat >= needed
                } else {
                    let _ = m;
                    true
                }
            }
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
        self.db.list_categories().ok()?.into_iter().find(|c| c.name == name)
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
        let global_running = tasks.iter().filter(|t| t.status == Status::Downloading).count();
        let q_running = tasks.iter().filter(|t| t.status == Status::Downloading && t.queue == task.queue).count();
        let queues = self.db.list_queues().unwrap_or_default();
        let q_enabled = queues.iter().find(|q| q.name == task.queue).map(|q| q.enabled).unwrap_or(true);
        let q_max = queues.iter().find(|q| q.name == task.queue).map(|q| q.max_simultaneous as usize).unwrap_or(3);

        // Disabled queue: park as Queued. `set_queue_enabled(true)` will
        // re-run `govern_concurrency`, which will promote it.
        if !q_enabled {
            let _ = self.db.set_status(task.id, Status::Queued, None);
            let _ = self.events.send(EngineEvent::StatusChanged { id: task.id, status: Status::Queued, error: None });
            return;
        }

        if (global_max != 0 && global_running >= global_max) || q_running >= q_max {
            let _ = self.db.set_status(task.id, Status::Queued, None);
            let _ = self.events.send(EngineEvent::StatusChanged { id: task.id, status: Status::Queued, error: None });
            return;
        }

        let _ = self.db.set_status(task.id, Status::Downloading, None);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(task.id, cancel.clone());
        let me = self.clone();
        let tx = self.events.clone();
        let task_id = task.id;
        let started = now();
        tokio::spawn(async move {
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
        let is_original = task.format_id.as_deref() == Some("original");
        if task.format_id.is_some() && !is_original {
            let proxy = self.settings().proxy_url;
            let path = YtDlp::download(
                &task.url,
                task.format_id.as_deref().unwrap_or("bv*+ba/b"),
                Path::new(&task.save_dir),
                task.referer.as_deref(),
                task.user_agent.as_deref(),
                task.cookies.as_deref(),
                proxy.as_deref(),
                cancel.clone(),
                tx.clone(),
            )
            .await
            .map_err(TaskError::from)?;
            let _ = path;
            return Ok(0);
        }
        self.downloader.run(task, cancel, tx).await.map_err(TaskError::from)
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
                // Single atomic write: downloaded + total + status +
                // completed_at all flip together. No torn state on crash.
                let _ = self.db.finish_complete(id, *n as i64, total);
                // Integrity verification if a hash is configured.
                if let Some(task) = db_task {
                    if !task.checksum_value.is_empty() || task.expected_sha256.is_some() {
                        if let Some(final_path) = self.final_path(&task) {
                            if let Ok(got) = hash_file(&final_path, &task.checksum_algorithm) {
                                let expected = task
                                    .expected_sha256
                                    .clone()
                                    .filter(|s| !s.is_empty())
                                    .or_else(|| Some(task.checksum_value.clone()))
                                    .unwrap_or_default();
                                let ok = !expected.is_empty() && got.eq_ignore_ascii_case(&expected);
                                let _ = self.db.set_meta(id, &DownloadMeta {
                                    checksum_verified: ok,
                                    checksum_value: got,
                                    checksum_algorithm: task.checksum_algorithm.clone(),
                                    ..DownloadMeta::from(&task)
                                });
                            }
                        }
                    }
                }
                (Status::Completed, None)
            }
            Err(TaskError::Cancelled) => (
                user_final.unwrap_or(Status::Paused),
                None,
            ),
            Err(e) => (Status::Failed, Some(Self::error_string(&e))),
        };

        // For non-Completed paths the status flip is its own atomic write.
        if status != Status::Completed {
            let _ = self.db.finish_terminal(id, status, err.as_deref());
        }
        if let Some(task) = self.db.get(id).unwrap_or(None) {
            let _ = self.events.send(EngineEvent::Finished { task: task.clone() });
            let _ = self.events.send(EngineEvent::StatusChanged { id, status, error: err.clone() });

            // Completion actions (only on success).
            if status == Status::Completed {
                self.run_completion_actions(&task);
            }
            // Queue empty → run queue completion action.
            if status == Status::Completed || status == Status::Failed || status == Status::Cancelled {
                self.maybe_queue_completed(&task.queue);
            }
        }

        // Attempt auto-retry for retryable failures.
        if status == Status::Failed {
            self.maybe_retry(id);
        }

        // Free up a concurrency slot and promote next queued.
        self.govern_concurrency();
        let _ = started;
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
            error: Some(format!("Retrying (attempt {} of {})…", task.retry_count + 1, max_retries)),
        });
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
            if let Some(t) = me.db.get(id).unwrap_or(None) {
                if t.status == Status::Queued {
                    me.start(t);
                }
            }
        });
    }

    fn maybe_queue_completed(self: &Arc<Self>, queue: &str) {
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
        if q.on_complete == "none" || q.on_complete_command.is_empty() && q.on_complete == "none" {
            return;
        }
        match q.on_complete.as_str() {
            "shutdown" => { let _ = std::process::Command::new("systemctl").args(["poweroff"]).spawn(); }
            "suspend" => { let _ = std::process::Command::new("systemctl").args(["suspend"]).spawn(); }
            "hibernate" => { let _ = std::process::Command::new("systemctl").args(["hibernate"]).spawn(); }
            "run" if !q.on_complete_command.is_empty() => {
                let _ = std::process::Command::new("sh").arg("-c").arg(&q.on_complete_command).spawn();
            }
            _ => {}
        }
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
        match spec.action.as_str() {
            "open_file" => {
                if let Some(p) = self.final_path(task) {
                    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
                }
            }
            "open_folder" => {
                let _ = std::process::Command::new("xdg-open").arg(&task.save_dir).spawn();
            }
            "run_command" if !spec.command.is_empty() => {
                let cmd = spec.command.replace("{file}", &self.final_path(task).unwrap_or_default());
                let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
            }
            "start_queue" if !spec.target_queue.is_empty() => {
                if let Some(q) = self.db.get_queue(&spec.target_queue).unwrap_or(None) {
                    let q = DownloadQueue { enabled: true, ..q };
                    let _ = self.db.upsert_queue(&q);
                    self.govern_concurrency();
                }
            }
            "shutdown" => {
                let _ = std::process::Command::new("systemctl").args(["poweroff"]).spawn();
            }
            "suspend" => {
                let _ = std::process::Command::new("systemctl").args(["suspend"]).spawn();
            }
            "hibernate" => {
                let _ = std::process::Command::new("systemctl").args(["hibernate"]).spawn();
            }
            _ => {}
        }
        let _ = self.events.send(EngineEvent::Notify {
            level: "success".into(),
            title: "Download complete".into(),
            body: task.filename.clone(),
        });
    }

    fn final_path(self: &Arc<Self>, task: &DownloadTask) -> Option<String> {
        let p = Path::new(&task.save_dir).join(&task.filename);
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    pub fn pause(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        match task.status {
            // Queued / Scheduled → mark paused without ever starting.
            Status::Queued | Status::Scheduled => {
                let _ = self.db.set_status(id, Status::Paused, None);
                let _ = self
                    .events
                    .send(EngineEvent::StatusChanged { id, status: Status::Paused, error: None });
                return Ok(());
            }
            // Stopped / Failed → no active download to interrupt; keep state.
            Status::Stopped | Status::Failed | Status::Paused | Status::Completed => return Ok(()),
            Status::Downloading => {}
            Status::Cancelled => {
                // Cancel race already finished; revive to paused.
                let _ = self.db.set_status(id, Status::Paused, None);
                let _ = self
                    .events
                    .send(EngineEvent::StatusChanged { id, status: Status::Paused, error: None });
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
        let _ = self.events.send(EngineEvent::StatusChanged { id, status: Status::Paused, error: None });
        self.govern_concurrency();
        Ok(())
    }

    pub fn resume(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        if !task.status.is_resumable() {
            return Err(format!(
                "only paused, stopped, or failed tasks can be resumed (current: {})",
                task.status.as_str()
            ));
        }
        // Reset error category on manual resume from failure.
        if task.status == Status::Failed {
            let _ = self.db.set_meta(id, &DownloadMeta {
                error_category: String::new(),
                ..DownloadMeta::from(&task)
            });
        }
        // Stamp the resume time so the UI can show "last resumed" without
        // polling the DB on every render.
        let _ = self.db.set_last_resumed(id);
        self.start(task);
        Ok(())
    }

    pub fn cancel(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        match task.status {
            // Not yet started → mark stopped, no download to interrupt.
            Status::Queued | Status::Scheduled => {
                let _ = self.db.set_status(id, Status::Stopped, None);
                let _ = self
                    .events
                    .send(EngineEvent::StatusChanged { id, status: Status::Stopped, error: None });
                return Ok(());
            }
            // Already in a terminal / paused state — no-op.
            Status::Stopped | Status::Failed | Status::Paused | Status::Completed | Status::Cancelled => {
                return Ok(())
            }
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
        let _ = self.events.send(EngineEvent::StatusChanged { id, status: Status::Stopped, error: None });
        self.govern_concurrency();
        Ok(())
    }

    /// Re-check existing partial data on disk and resync the task.
    /// For chunked downloads, the data is split into `parts-{id}/part{0..n}`;
    /// for single-stream downloads the file lives at the final path. Sum
    /// both so the resumed size is the true on-disk total.
    pub fn recheck(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
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
            let total = if task.total_size > 0 { task.total_size } else { existing as i64 };
            let _ = self.db.update_progress(id, existing as i64, existing as i64, total, 0.0);
        }
        let _ = self.events.send(EngineEvent::Finished { task: self.db.get(id).unwrap_or(None).unwrap_or(task) });
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
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        let path = Path::new(&task.save_dir).join(&task.filename);
        std::fs::remove_file(&path).map_err(|e| format!("cannot delete file: {e}"))?;
        let _ = self.events.send(EngineEvent::Notify { level: "info".into(), title: "File deleted".into(), body: task.filename });
        Ok(())
    }

    pub fn redownload(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        // Remove partial + final file then re-add fresh.
        let part_dir = self.state_dir.join(format!("parts-{id}"));
        let _ = std::fs::remove_dir_all(&part_dir);
        let path = Path::new(&task.save_dir).join(&task.filename);
        let _ = std::fs::remove_file(&path);
        let _ = self.db.set_status(id, Status::Queued, None);
        let _ = self.db.update_progress(id, 0, 0, task.total_size, 0.0);
        let _ = self.events.send(EngineEvent::StatusChanged { id, status: Status::Queued, error: None });
        self.start(task);
        Ok(())
    }

    pub fn move_to_queue(self: &Arc<Self>, id: i64, queue: &str) -> Result<(), String> {
        self.db.set_queue(id, queue).map_err(|e| e.to_string())?;
        self.govern_concurrency();
        Ok(())
    }

    pub fn set_category(self: &Arc<Self>, id: i64, category: &str) -> Result<(), String> {
        self.db.set_category(id, category).map_err(|e| e.to_string())?;
        // Apply category save dir if configured.
        if let Some(cat) = self.category(category) {
            if !cat.save_dir.is_empty() {
                let _ = self.db.set_save_dir(id, &cat.save_dir);
            }
        }
        Ok(())
    }

    pub fn rename(self: &Arc<Self>, id: i64, filename: &str) -> Result<(), String> {
        self.db.set_filename(id, filename).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_save_dir_for(self: &Arc<Self>, id: i64, dir: &str) -> Result<(), String> {
        self.db.set_save_dir(id, dir).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_description(self: &Arc<Self>, id: i64, desc: &str) -> Result<(), String> {
        self.db.set_description(id, desc).map_err(|e| e.to_string())?;
        Ok(())
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
        self.db.upsert_queue(&q).map_err(|e| e.to_string())?;
        let _ = self.events.send(EngineEvent::QueueChanged { queue: q });
        self.govern_concurrency();
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
                Status::Paused | Status::Stopped | Status::Failed | Status::Queued | Status::Scheduled
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
            ) {
                if self.cancel(t.id).is_ok() {
                    stopped += 1;
                }
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

    pub async fn import_downloads(self: &Arc<Self>, path: &str, start: bool) -> Result<usize, String> {
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
                browser_dedup_id: String::new(),
                skip_probe: false,
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
            if matches!(t.status, Status::Completed | Status::Cancelled | Status::Stopped) {
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
                self.downloader.set_proxy(if value.is_empty() { None } else { Some(value) });
            }
            _ => {}
        }
    }

    pub fn set_queue_speed(self: &Arc<Self>, queue: &str, kbps: u64) {
        self.downloader.set_queue_speed(queue.to_string(), kbps * 1024);
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

        let attr_re = regex::Regex::new(r#"(?i)(?:href|src)\s*=\s*["']([^"']+)["']"#).unwrap();
        let ext_re = regex::Regex::new(
            r"(?i)\.(zip|rar|7z|tar|gz|bz2|xz|iso|img|exe|msi|apk|deb|rpm|appimage|dmg|sh|run|mp3|flac|wav|m4a|ogg|opus|mp4|mkv|webm|avi|mov|wmv|flv|m4v|ts|mpg|pdf|epub|djvu|docx?|xlsx?|pptx?|csv|sql|db|torrent)(\?|$)",
        )
        .unwrap();

        let mut seen = std::collections::HashSet::new();
        let mut links = Vec::new();
        for cap in attr_re.captures_iter(&html) {
            let raw = cap[1].trim();
            if raw.is_empty() || raw.starts_with('#') || raw.starts_with("mailto:") || raw.starts_with("javascript:") {
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
            let ext = filename.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
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
    pub async fn grab_site(self: &Arc<Self>, url: &str, limit: usize) -> Result<Vec<GrabbedLink>, String> {
        self.grab_site_with_rules(url, limit, &crate::models::GrabRules::sensible_defaults()).await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the hostname of a URL, lowercased, with any `:port` stripped.
/// Returns empty string on parse failure (so callers can decide whether
/// to allow the empty host through).
fn host_of(url: &str) -> String {
    let after = url.splitn(2, "://").nth(1).unwrap_or(url);
    let host_port = after.splitn(2, '/').next().unwrap_or(after);
    let host_port = host_port.splitn(2, '?').next().unwrap_or(host_port);
    let host_port = host_port.splitn(2, '#').next().unwrap_or(host_port);
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
    if !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '*')
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

fn domain_of(url: &str) -> String {
    url.split("//").nth(1).and_then(|s| s.split('/').next()).unwrap_or("").replace("www.", "")
}

fn classify_error(e: &TaskError) -> ErrorClass {
    match e {
        TaskError::Cancelled => ErrorClass::Cancelled,
        TaskError::Download(d) => classify_download(d),
        TaskError::YtDlp(y) => classify_ytdlp(y),
    }
}

fn classify_download(d: &crate::download::DownloadError) -> ErrorClass {
    use crate::download::DownloadError::*;
    match d {
        ConnectionFailed(_) => ErrorClass::Connection,
        Timeout(_) => ErrorClass::Timeout,
        DnsFailed(_) => ErrorClass::Dns,
        TlsError(_) => ErrorClass::Tls,
        HttpStatus(c) => crate::models::classify_http(*c),
        DiskFull => ErrorClass::DiskFull,
        Permission(_) => ErrorClass::Permission,
        BadDestination(_) => ErrorClass::BadDestination,
        ChecksumMismatch { .. } => ErrorClass::Checksum,
        Cancelled => ErrorClass::Cancelled,
        _ => ErrorClass::Unknown,
    }
}

fn classify_ytdlp(y: &crate::ytdlp::YtDlpError) -> ErrorClass {
    use crate::ytdlp::YtDlpError::*;
    match y {
        Timeout(_) => ErrorClass::Timeout,
        ConnectionFailed(_) => ErrorClass::Connection,
        NotFound(_) => ErrorClass::NotFound,
        Forbidden(_) => ErrorClass::Forbidden,
        _ => ErrorClass::Unknown,
    }
}

fn is_retryable(cat: &str) -> bool {
    matches!(cat, "dns" | "connection" | "timeout" | "tls" | "429" | "5xx" | "unknown")
}

#[cfg(target_os = "linux")]
fn nix_statfs(_dir: &str) -> Result<i64, ()> {
    // Best-effort; without nix crate we approximate by trusting the OS.
    Err(())
}

#[cfg(not(target_os = "linux"))]
fn nix_statfs(_dir: &str) -> Result<i64, ()> {
    Err(())
}

fn human_bytes(n: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, units[i])
}

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
    if matches!(e.as_str(), "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso") {
        "compressed".into()
    } else if matches!(e.as_str(), "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "m4v" | "ts" | "mpg") {
        "video".into()
    } else if matches!(e.as_str(), "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus") {
        "music".into()
    } else if matches!(e.as_str(), "pdf" | "epub" | "doc" | "docx" | "txt" | "md" | "djvu") {
        "documents".into()
    } else if matches!(e.as_str(), "exe" | "msi" | "deb" | "rpm" | "appimage" | "apk" | "dmg" | "run" | "sh") {
        "programs".into()
    } else {
        "other".into()
    }
}

/// Apply user-defined `GrabRules` to a candidate link. Returns
/// `Ok(matched_reason)` if the link should be kept, `Err(drop_reason)`
/// otherwise. Both `Ok` and `Err` carry a short string the UI can
/// display in a tooltip.
fn grabber_filter(url: &str, ext: &str, rules: &crate::models::GrabRules) -> Result<String, String> {
    let ext_lower = ext.to_ascii_lowercase();
    // 1. exclude_extensions always wins.
    if rules.exclude_extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext_lower)) {
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
        && !rules.include_extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext_lower))
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
    let reason = if rules.include_extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext_lower)) {
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
    let end_digits: String = after_dots.chars().take_while(|c| c.is_ascii_digit()).collect();
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

#[cfg(test)]
mod batch_parse_tests {
    use super::*;

    #[test]
    fn parses_one_url_per_line() {
        let urls = parse_batch_urls(
            "https://example.com/a.zip\nhttps://example.com/b.zip\n\n# comment\nhttps://example.com/c.zip",
        );
        assert_eq!(urls, vec![
            "https://example.com/a.zip",
            "https://example.com/b.zip",
            "https://example.com/c.zip",
        ]);
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
        assert_eq!(urls, vec![
            "https://example.com/file-001.zip",
            "https://example.com/file-002.zip",
            "https://example.com/file-003.zip",
        ]);
    }

    #[test]
    fn expands_unpadded_range() {
        let urls = parse_batch_urls("https://example.com/file-1..3.zip");
        assert_eq!(urls, vec![
            "https://example.com/file-1.zip",
            "https://example.com/file-2.zip",
            "https://example.com/file-3.zip",
        ]);
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
        assert_eq!(urls, vec![
            "https://example.com/abc-1-xyz.zip",
            "https://example.com/abc-2-xyz.zip",
        ]);
    }
}

/// Resolve a possibly-relative URL against the page URL.
fn resolve_url(base: &str, raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_string();
    }
    if raw.starts_with("//") {
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
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
