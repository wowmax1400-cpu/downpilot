//! DownloadManager: owns the task registry (SQLite), spawns chunked
//! downloads or yt-dlp media downloads, handles pause/resume/cancel, and
//! pushes events out through a pump loop to the UI layer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Timelike;
use tokio::sync::mpsc;

use crate::db::Db;
use crate::download::{ChunkDownloader, DownloadError};
use crate::models::{AppSettings, DownloadTask, EngineEvent, GrabbedLink, MediaFormat, MediaInfo, NewDownload, Status};
use crate::ytdlp::{YtDlp, YtDlpError};

pub struct DownloadManager {
    db: Db,
    state_dir: PathBuf,
    speed_limit: Arc<AtomicU64>,
    downloader: Arc<ChunkDownloader>,
    /// Internal event channel; the pump loop reads it, persists progress,
    /// and forwards to the UI sink.
    events: mpsc::UnboundedSender<EngineEvent>,
    /// task_id -> cancellation flag
    cancels: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    /// Tasks auto-paused by the schedule window (not user-initiated).
    auto_paused: Mutex<HashSet<i64>>,
}

impl DownloadManager {
    /// Creates the manager plus the internal event receiver that the pump
    /// loop consumes.
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
                                continue; // yt-dlp progress without a task id
                            }
                            let now = Instant::now();
                            let speed = match speeds.get(&id) {
                                Some((last_t, last_b)) => {
                                    let dt = now.duration_since(*last_t).as_secs_f64();
                                    if dt >= 0.4 {
                                        let s = ((*downloaded - *last_b) as f64 / dt).max(0.0);
                                        speeds.insert(*id, (now, *downloaded));
                                        s
                                    } else {
                                        // keep previous speed
                                        speeds.get(&id).map(|_| 0.0).unwrap_or(0.0)
                                    }
                                }
                                None => {
                                    speeds.insert(*id, (now, *downloaded));
                                    0.0
                                }
                            };
                            let _ = self.db.update_progress(*id, *downloaded, *total);
                            EngineEvent::Progress { id: *id, downloaded: *downloaded, total: *total, speed_bps: speed }
                        }
                        EngineEvent::StatusChanged { id, status, error } => {
                            let _ = self.db.set_status(*id, *status, error.as_deref());
                            ev.clone()
                        }
                        _ => ev.clone(),
                    };
                    let _ = sink.send(fwd);
                }

                // ---- Schedule window enforcement ---------------------------
                self.enforce_schedule();
            }
        });
    }

    /// Pause/resume downloads to match the configured daily schedule window.
    /// Runs every pump tick (~400ms). Tasks auto-paused here are remembered
    /// and resumed automatically when the window re-opens; user-initiated
    /// pauses/cancels are never overridden.
    fn enforce_schedule(self: &Arc<Self>) {
        let s = self.settings();
        let enabled = s.schedule_enabled.unwrap_or(false);
        let (Some(start), Some(end)) = (s.schedule_start, s.schedule_end) else {
            return;
        };
        let in_window = Self::time_in_window(&start, &end);
        let tasks = self.list();

        if !enabled {
            // Schedule turned off: resume anything we auto-paused.
            let ids: Vec<i64> = self.auto_paused.lock().unwrap().drain().collect();
            for id in ids {
                if let Some(t) = tasks.iter().find(|t| t.id == id) {
                    if t.status == Status::Paused {
                        self.start(t.clone());
                    }
                }
            }
            return;
        }

        if in_window {
            // Resume tasks we auto-paused when the window opened.
            let ids: Vec<i64> = self.auto_paused.lock().unwrap().drain().collect();
            for id in ids {
                if let Some(t) = tasks.iter().find(|t| t.id == id) {
                    if t.status == Status::Paused {
                        self.start(t.clone());
                    }
                }
            }
        } else {
            // Outside the window: auto-pause active downloads (remember them).
            for t in tasks {
                if t.status == Status::Downloading {
                    let _ = self.pause(t.id);
                    self.auto_paused.lock().unwrap().insert(t.id);
                }
            }
        }
    }

    /// Is the current local time inside a "HH:MM"–"HH:MM" window? Handles
    /// overnight windows (start > end, e.g. 23:00–06:00). Uses the system
    /// local timezone (chrono), so it follows the user's clock.
    fn time_in_window(start: &str, end: &str) -> bool {
        let now_min = {
            let now = chrono::Local::now();
            (now.hour() * 60 + now.minute()) as i64
        };
        let parse = |s: &str| -> Option<i64> {
            let mut it = s.split(':');
            let h: i64 = it.next()?.trim().parse().ok()?;
            let m: i64 = it.next()?.trim().parse().ok()?;
            Some(h * 60 + m)
        };
        let (Some(s), Some(e)) = (parse(start), parse(end)) else {
            return true; // malformed schedule → don't block downloads
        };
        if s <= e {
            now_min >= s && now_min < e
        } else {
            now_min >= s || now_min < e // overnight
        }
    }

    pub fn list(&self) -> Vec<DownloadTask> {
        self.db.list().unwrap_or_default()
    }

    pub fn get(&self, id: i64) -> Option<DownloadTask> {
        self.db.get(id).unwrap_or(None)
    }

    pub fn default_save_dir(&self) -> String {
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
    pub fn add(self: &Arc<Self>, nd: NewDownload) -> Result<i64, String> {
        let save_dir = nd.save_dir.clone().unwrap_or_else(|| self.default_save_dir());
        if !Path::new(&save_dir).exists() {
            std::fs::create_dir_all(&save_dir).map_err(|e| format!("cannot create save dir: {e}"))?;
        }
        let id = self.db.insert(&nd, &save_dir).map_err(|e| e.to_string())?;
        if let Some(task) = self.db.get(id).unwrap_or(None) {
            let _ = self.events.send(EngineEvent::Added { task: task.clone() });
            if nd.start_immediately {
                self.start(task);
            }
        }
        Ok(id)
    }

    pub fn start(self: &Arc<Self>, task: DownloadTask) {
        if task.status == Status::Downloading || task.status == Status::Completed {
            return;
        }
        let _ = self.db.set_status(task.id, Status::Downloading, None);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(task.id, cancel.clone());
        let me = self.clone();
        let tx = self.events.clone();
        let task_id = task.id;
        tokio::spawn(async move {
            let result = me.run_task(task, cancel, tx).await;
            me.finish_task(task_id, result);
        });
        let _ = self.events.send(EngineEvent::StatusChanged {
            id: task_id,
            status: Status::Downloading,
            error: None,
        });
    }

    async fn run_task(
        &self,
        task: DownloadTask,
        cancel: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<u64, TaskError> {
        // "original" = direct-file fallback from probe_direct: use the
        // chunked downloader, not yt-dlp (yt-dlp may refuse the site).
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

    fn finish_task(&self, id: i64, result: Result<u64, TaskError>) {
        self.cancels.lock().unwrap().remove(&id);
        let (status, err) = match &result {
            Ok(n) => {
                // Persist final downloaded/total directly from the result —
                // the pump loop only persists every ~400ms and a fast
                // download may finish before any progress event was emitted.
                let db_task = self.db.get(id).unwrap_or(None);
                let total = db_task
                    .as_ref()
                    .map(|t| t.total_size.max(*n as i64))
                    .unwrap_or(*n as i64);
                let _ = self.db.update_progress(id, *n as i64, total);
                (Status::Completed, None)
            }
            Err(TaskError::Cancelled) => (Status::Paused, None),
            Err(e) => (Status::Failed, Some(e.to_string())),
        };
        let _ = self.db.set_status(id, status, err.as_deref());
        let _ = self.events.send(EngineEvent::StatusChanged { id, status, error: err });
    }

    pub fn pause(&self, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        if task.status != Status::Downloading {
            return Err("task is not downloading".into());
        }
        if let Some(cancel) = self.cancels.lock().unwrap().get(&id) {
            cancel.store(true, Ordering::Relaxed);
        }
        // A manual pause cancels any schedule auto-pause claim on this task.
        self.auto_paused.lock().unwrap().remove(&id);
        let _ = self.db.set_status(id, Status::Paused, None);
        let _ = self.events.send(EngineEvent::StatusChanged { id, status: Status::Paused, error: None });
        Ok(())
    }

    pub fn resume(self: &Arc<Self>, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        if task.status != Status::Paused && task.status != Status::Failed {
            return Err("only paused or failed tasks can be resumed".into());
        }
        self.start(task);
        Ok(())
    }

    pub fn cancel(&self, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        if task.status == Status::Downloading {
            self.pause(id)?;
        }
        let _ = self.db.set_status(id, Status::Cancelled, None);
        let _ = self.events.send(EngineEvent::StatusChanged { id, status: Status::Cancelled, error: None });
        Ok(())
    }

    pub fn remove(&self, id: i64) -> Result<(), String> {
        let task = self.db.get(id).map_err(|e| e.to_string())?.ok_or("task not found")?;
        if task.status == Status::Downloading {
            let _ = self.pause(id);
        }
        self.db.delete(id).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(self.state_dir.join(format!("parts-{id}")));
        let _ = self.events.send(EngineEvent::Removed { id });
        Ok(())
    }

    /// Analyze a URL with yt-dlp and return curated formats.
    ///
    /// If yt-dlp refuses or fails (e.g. sites it classifies as piracy, or
    /// geoblocked extractors), falls back to a direct-file probe: when the
    /// URL serves a media content-type, an "original" quick-pick is returned
    /// so the chunked downloader can grab it without yt-dlp.
    pub async fn analyze(
        &self,
        url: &str,
        cookies: Option<&str>,
        referer: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<MediaInfo, String> {
        if !YtDlp::is_available() {
            return Err("yt-dlp is not installed on this system".into());
        }
        let opts = crate::models::ProbeOptions {
            cookies: cookies.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            referer: referer.map(|s| s.to_string()),
            proxy: self.settings().proxy_url,
        };
        match YtDlp::probe(url, &opts).await {
            Ok(info) => Ok(info),
            Err(e) => {
                // yt-dlp can't handle this URL (refused site, unsupported
                // extractor, ...). If the URL itself serves media, fall back
                // to a direct probe so the download still works.
                match self.probe_direct(url, referer, user_agent, cookies).await {
                    Some(info) => Ok(info),
                    None => Err(e.to_string()),
                }
            }
        }
    }

    /// Probe a URL as a direct file. Returns `None` when the URL does not
    /// look like directly-downloadable media (e.g. an HTML page).
    async fn probe_direct(
        &self,
        url: &str,
        referer: Option<&str>,
        user_agent: Option<&str>,
        cookies: Option<&str>,
    ) -> Option<MediaInfo> {
        let client = self.downloader.http_client();
        let mut req = client.head(url);
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        if let Some(ua) = user_agent {
            req = req.header("User-Agent", ua);
        } else {
            req = req.header("User-Agent", "DownPilot/0.1 (direct probe)");
        }
        if let Some(c) = cookies {
            req = req.header("Cookie", c);
        }
        let resp = tokio::time::timeout(Duration::from_secs(20), req.send())
            .await
            .ok()?
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let is_media = content_type.starts_with("video/") || content_type.starts_with("audio/");
        if !is_media {
            return None;
        }
        let ext = content_type
            .split('/')
            .nth(1)
            .unwrap_or("bin")
            .split(';')
            .next()
            .unwrap_or("bin")
            .to_string();
        let filesize = resp.content_length().map(|n| n as i64);
        let title = url
            .rsplit('/')
            .next()
            .map(|s| s.split('?').next().unwrap_or(s))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "video".into());
        let fmt = MediaFormat {
            format_id: "original".into(),
            ext: ext.clone(),
            resolution: Some("Original".into()),
            fps: None,
            vcodec: None,
            acodec: None,
            filesize,
            note: Some("direct file".into()),
            url: Some(url.to_string()),
            protocol: Some("http".into()),
        };
        Some(MediaInfo {
            title,
            webpage_url: url.to_string(),
            thumbnail: None,
            duration: None,
            uploader: None,
            formats: vec![fmt.clone()],
            quick_picks: vec![fmt],
            default_filename: format!("video.{ext}"),
        })
    }
    pub fn set_speed_limit(&self, kbps: u64) {
        self.speed_limit.store(kbps.saturating_mul(1024), Ordering::Relaxed);
        let _ = self.db.set_setting("speed_limit_kbps", &kbps.to_string());
    }

    pub fn speed_limit(&self) -> u64 {
        self.speed_limit.load(Ordering::Relaxed) / 1024
    }

    pub fn settings(&self) -> AppSettings {
        self.db.all_settings().unwrap_or_default()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let _ = self.db.set_setting(key, value);
        if key == "speed_limit_kbps" {
            if let Ok(v) = value.parse::<u64>() {
                self.set_speed_limit(v);
            }
        }
        if key == "proxy_url" {
            self.downloader.set_proxy(if value.is_empty() { None } else { Some(value) });
        }
    }

    /// Fetch a page and extract direct file links (site grabber).
    /// Returns up to `limit` links matching common downloadable extensions.
    pub async fn grab_site(&self, url: &str, limit: usize) -> Result<Vec<GrabbedLink>, String> {
        let client = self.downloader.http_client();
        let resp = client
            .get(url)
            .header("User-Agent", "DownPilot/0.1 (site grabber)")
            .send()
            .await
            .map_err(|e| format!("fetch failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("server returned {}", resp.status()));
        }
        let html = resp.text().await.map_err(|e| format!("read failed: {e}"))?;

        // Strip tags, then find href/src attributes that look like files.
        let attr_re = regex::Regex::new(r#"(?:href|src)\s*=\s*["']([^"']+)["']"#).unwrap();
        let ext_re = regex::Regex::new(
            r"(?i)\.(zip|rar|7z|tar|gz|bz2|xz|iso|img|exe|msi|apk|deb|rpm|mp3|flac|wav|m4a|ogg|mp4|mkv|webm|avi|mov|pdf|epub|djvu|docx?|xlsx?|pptx?|csv|sql|db|torrent)(\?|$)",
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
            let absolute = resolve_url(url, raw);
            if seen.insert(absolute.clone()) {
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
                    .to_lowercase();
                links.push(GrabbedLink { url: absolute, filename, ext });
                if links.len() >= limit {
                    break;
                }
            }
        }
        Ok(links)
    }
}

#[derive(Debug)]
pub enum TaskError {
    Download(DownloadError),
    YtDlp(YtDlpError),
    Cancelled,
}

impl From<DownloadError> for TaskError {
    fn from(e: DownloadError) -> Self {
        if matches!(e, DownloadError::Cancelled) {
            TaskError::Cancelled
        } else {
            TaskError::Download(e)
        }
    }
}

impl From<YtDlpError> for TaskError {
    fn from(e: YtDlpError) -> Self {
        if matches!(e, YtDlpError::Cancelled) {
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

/// Resolve a possibly-relative URL against the page URL.
fn resolve_url(base: &str, raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return raw.to_string();
    }
    if raw.starts_with("//") {
        // protocol-relative: inherit the page scheme
        let scheme = if base.starts_with("https") { "https:" } else { "http:" };
        return format!("{scheme}{raw}");
    }
    let base_trim = base.split(['?', '#']).next().unwrap_or(base);
    let base_dir = base_trim.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    if raw.starts_with('/') {
        // absolute path: scheme + host + path
        if let Some((scheme_host, _)) = base_trim.split_once('/') {
            return format!("{scheme_host}{raw}");
        }
        return raw.to_string();
    }
    format!("{base_dir}/{raw}")
}
