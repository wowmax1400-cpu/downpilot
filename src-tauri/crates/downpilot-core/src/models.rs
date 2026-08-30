//! Data models for DownPilot: download tasks, queue/category/site/schedule
//! management structures, settings, and result types. IDM-class feature set.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Scheduled,
    Downloading,
    Paused,
    Stopped,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Scheduled => "scheduled",
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Stopped => "stopped",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Cancelled => "cancelled",
        }
    }
    /// True when the task is actively transferring bytes.
    pub fn is_active(&self) -> bool {
        matches!(self, Status::Downloading)
    }
    /// True when the task can be resumed from where it left off.
    /// Paused and Stopped both keep partial files; Failed usually does too
    /// unless `redownload` was explicitly called.
    pub fn is_resumable(&self) -> bool {
        matches!(self, Status::Paused | Status::Stopped | Status::Failed)
    }
    /// True when the user is allowed to start the task from the UI.
    pub fn is_startable(&self) -> bool {
        matches!(
            self,
            Status::Queued | Status::Scheduled | Status::Paused | Status::Stopped | Status::Failed
        )
    }
}

/// Classified error category used for retry policy and user-facing messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ErrorClass {
    #[default]
    None,
    Cancelled,
    Connection,
    Timeout,
    Dns,
    Tls,
    HttpStatus,
    Permission,
    BadDestination,
    DiskFull,
    Checksum,
    NotFound,
    Forbidden,
    Unknown,
}

impl ErrorClass {
    /// Whether this error should be retried automatically.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            ErrorClass::Connection
                | ErrorClass::Timeout
                | ErrorClass::Dns
                | ErrorClass::Tls
                | ErrorClass::HttpStatus
                | ErrorClass::Forbidden
                | ErrorClass::NotFound
                | ErrorClass::Unknown
        )
    }
    pub fn label(&self) -> &'static str {
        match self {
            ErrorClass::None => "",
            ErrorClass::Cancelled => "Cancelled",
            ErrorClass::Connection => "Connection failed",
            ErrorClass::Timeout => "Connection timed out",
            ErrorClass::Dns => "DNS resolution failed",
            ErrorClass::Tls => "TLS / certificate error",
            ErrorClass::HttpStatus => "HTTP error",
            ErrorClass::Permission => "Permission denied",
            ErrorClass::BadDestination => "Invalid destination path",
            ErrorClass::DiskFull => "Disk full",
            ErrorClass::Checksum => "Checksum mismatch",
            ErrorClass::NotFound => "File not found",
            ErrorClass::Forbidden => "Access forbidden",
            ErrorClass::Unknown => "Unknown error",
        }
    }
    /// Lowercase category string used in the DB `error_category` column and
    /// retry policy. Must stay in sync with `is_retryable`.
    pub fn category(&self) -> &'static str {
        match self {
            ErrorClass::None => "",
            ErrorClass::Cancelled => "cancelled",
            ErrorClass::Connection => "connection",
            ErrorClass::Timeout => "timeout",
            ErrorClass::Dns => "dns",
            ErrorClass::Tls => "tls",
            ErrorClass::HttpStatus => "5xx",
            ErrorClass::Permission => "permission",
            ErrorClass::BadDestination => "bad_destination",
            ErrorClass::DiskFull => "disk_full",
            ErrorClass::Checksum => "checksum",
            ErrorClass::NotFound => "404",
            ErrorClass::Forbidden => "403",
            ErrorClass::Unknown => "unknown",
        }
    }
}

/// Map an HTTP status code to an ErrorClass.
pub fn classify_http(code: u16) -> ErrorClass {
    match code {
        401 => ErrorClass::Forbidden,
        403 => ErrorClass::Forbidden,
        404 => ErrorClass::NotFound,
        407 => ErrorClass::Forbidden,
        429 => ErrorClass::HttpStatus,
        500..=599 => ErrorClass::HttpStatus,
        _ if (400..499).contains(&code) => ErrorClass::HttpStatus,
        _ => ErrorClass::Unknown,
    }
}

/// Completion actions executed when a download (or queue) finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompletionAction {
    #[default]
    None,
    OpenFile,
    OpenFolder,
    Notification,
    PlaySound,
    RunCommand,
    StartQueue,
    Shutdown,
    Suspend,
    Hibernate,
}

impl CompletionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            CompletionAction::None => "none",
            CompletionAction::OpenFile => "open_file",
            CompletionAction::OpenFolder => "open_folder",
            CompletionAction::Notification => "notification",
            CompletionAction::PlaySound => "play_sound",
            CompletionAction::RunCommand => "run_command",
            CompletionAction::StartQueue => "start_queue",
            CompletionAction::Shutdown => "shutdown",
            CompletionAction::Suspend => "suspend",
            CompletionAction::Hibernate => "hibernate",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "open_file" => CompletionAction::OpenFile,
            "open_folder" => CompletionAction::OpenFolder,
            "notification" => CompletionAction::Notification,
            "play_sound" => CompletionAction::PlaySound,
            "run_command" => CompletionAction::RunCommand,
            "start_queue" => CompletionAction::StartQueue,
            "shutdown" => CompletionAction::Shutdown,
            "suspend" => CompletionAction::Suspend,
            "hibernate" => CompletionAction::Hibernate,
            _ => CompletionAction::None,
        }
    }
}

/// A download task as stored in SQLite and shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: i64,
    pub url: String,
    pub filename: String,
    pub save_dir: String,
    pub status: Status,
    pub total_size: i64,
    pub downloaded: i64,
    pub transferred: i64,
    pub speed_bps: f64,
    pub threads: u32,
    pub format_id: Option<String>,
    pub webpage_url: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    /// Cookie header value for the original browser request. Forwarded
    /// to the engine's HTTP client so authenticated browser downloads
    /// complete without re-login. Stored on the row for the lifetime of
    /// the task only — never logged, never surfaced in the UI, never
    /// echoed in NMH responses. **Future hardening:** encrypt at rest
    /// using a machine-scoped key once the keyring backend is selected.
    pub cookies: Option<String>,
    /// Explicit per-download proxy override. `None` inherits the matching
    /// Site Rule, queue, global proxy, or direct connection.
    #[serde(default)]
    pub proxy_url: Option<String>,
    pub category: String,
    pub expected_sha256: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    // ---- IDM-class extensions ----
    pub queue: String,
    pub description: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub last_resumed_at: Option<i64>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub retry_delay_ms: i64,
    pub conn_timeout_ms: i64,
    pub read_timeout_ms: i64,
    pub peak_speed_bps: f64,
    pub avg_speed_bps: f64,
    pub segments: i32,
    pub resume_support: bool,
    pub http_status: i32,
    pub content_type: Option<String>,
    pub checksum_algorithm: String,
    pub checksum_value: String,
    pub checksum_verified: bool,
    pub site_id: String,
    pub error_code: i32,
    pub error_category: String,
    pub on_complete: String,
    /// Phase 3B: download-level priority inside the owning queue.
    /// High = +10, Normal = 0, Low = -10. Respected by
    /// `govern_concurrency` after queue priority but before
    /// creation time. Persisted via `Db::set_priority`.
    pub priority: i32,
    /// Phase 3B: true after the user explicitly pressed Force Start
    /// on this task. The UI shows a "Force Started" badge while it
    /// is set, but it does NOT change the task's permanent `priority`
    /// field — once the task transitions out of Downloading
    /// (finished / cancelled / paused for any reason), the flag is
    /// cleared so a future auto-promotion respects normal ordering.
    #[serde(default)]
    pub force_started: bool,
}

impl DownloadTask {
    pub fn progress_ratio(&self) -> f64 {
        if self.total_size > 0 {
            (self.downloaded as f64 / self.total_size as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    pub fn remaining(&self) -> i64 {
        (self.total_size - self.downloaded).max(0)
    }
    /// Construct a minimal `DownloadTask` for the downloader probe path —
    /// the engine doesn't own a real task yet at the moment it issues the
    /// probe to recover the server's filename. All other fields take
    /// sensible defaults; the caller is expected to fill in
    /// `user_agent` / `referer` / `cookies` from the real task before
    /// passing to `ChunkDownloader::probe`.
    pub fn default_for_probe(url: &str, filename: &str) -> Self {
        Self {
            id: 0,
            url: url.to_string(),
            filename: filename.to_string(),
            save_dir: String::new(),
            status: Status::Queued,
            total_size: 0,
            downloaded: 0,
            transferred: 0,
            speed_bps: 0.0,
            threads: 1,
            format_id: None,
            webpage_url: None,
            referer: None,
            user_agent: None,
            cookies: None,
            proxy_url: None,
            category: "other".to_string(),
            expected_sha256: None,
            error: None,
            created_at: 0,
            updated_at: 0,
            queue: "general".to_string(),
            description: None,
            started_at: None,
            completed_at: None,
            last_resumed_at: None,
            retry_count: 0,
            max_retries: 0,
            retry_delay_ms: 0,
            conn_timeout_ms: 0,
            read_timeout_ms: 0,
            peak_speed_bps: 0.0,
            avg_speed_bps: 0.0,
            segments: 0,
            resume_support: false,
            http_status: 0,
            content_type: None,
            checksum_algorithm: String::new(),
            checksum_value: String::new(),
            checksum_verified: false,
            site_id: String::new(),
            error_code: 0,
            error_category: String::new(),
            on_complete: String::new(),
            priority: 0,
            force_started: false,
        }
    }
}

/// Payload used to enqueue a new download from the UI or the extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDownload {
    pub url: String,
    pub filename: Option<String>,
    pub save_dir: Option<String>,
    pub threads: Option<u32>,
    pub format_id: Option<String>,
    pub webpage_url: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: Option<String>,
    /// Explicit per-download proxy override. Omit to inherit configuration.
    #[serde(default)]
    pub proxy_url: Option<String>,
    pub expected_sha256: Option<String>,
    pub category: Option<String>,
    pub queue: Option<String>,
    pub description: Option<String>,
    pub max_retries: Option<i32>,
    pub retry_delay_ms: Option<i64>,
    pub conn_timeout_ms: Option<i64>,
    pub read_timeout_ms: Option<i64>,
    pub checksum_algorithm: Option<String>,
    pub on_complete: Option<String>,
    #[serde(default = "default_true")]
    pub start_immediately: bool,
    /// Phase 3B: download-level priority (High=10, Normal=0, Low=-10).
    /// Falls back to the queue's default ordering (creation time) when
    /// `None`.
    #[serde(default)]
    pub priority: Option<i32>,
    /// Opaque browser-side handle used to dedupe: if a Chrome retry fires
    /// the same `chrome.downloads.onCreated` event with the same `browser_id`,
    /// the engine ignores the second insert. Empty string = not from a browser.
    #[serde(default)]
    pub browser_dedup_id: String,
    /// When true, the engine inserts the task but does NOT run the
    /// synchronous HEAD/ranged-GET probe. The downloader's normal byte-flow
    /// path will probe when it starts writing. Used by the "Try Download
    /// Anyway" branch of the browser-capture ask-notification when the
    /// initial probe failed (404, 403, slow server, etc.) but the user
    /// still wants to attempt the download.
    #[serde(default)]
    pub skip_probe: bool,
}

fn default_true() -> bool {
    true
}

/// Events the engine pushes to the UI layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { task: DownloadTask },
    Progress { id: i64, downloaded: i64, total: i64, speed_bps: f64 },
    StatusChanged { id: i64, status: Status, error: Option<String> },
    Removed { id: i64 },
    Finished { task: DownloadTask },
    Notify { level: String, title: String, body: String },
    QueueChanged { queue: DownloadQueue },
}

/// One selectable media format returned by yt-dlp analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<i64>,
    pub note: Option<String>,
    pub url: Option<String>,
    pub protocol: Option<String>,
}

/// Result of probing a media URL with yt-dlp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub title: String,
    pub webpage_url: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub uploader: Option<String>,
    pub formats: Vec<MediaFormat>,
    #[serde(rename = "best_formats")]
    pub quick_picks: Vec<MediaFormat>,
    pub default_filename: String,
}

/// Options for the yt-dlp probe.
#[derive(Debug, Clone, Default)]
pub struct ProbeOptions {
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub proxy: Option<String>,
}

/// Settings persisted in SQLite (key/value).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub download_dir: Option<String>,
    pub default_threads: Option<u32>,
    pub max_threads: Option<u32>,
    pub max_simultaneous: Option<u32>,
    pub speed_limit_kbps: Option<u64>,
    pub theme: Option<String>,
    pub accent: Option<String>,
    pub auto_intercept: Option<bool>,
    pub use_ytdlp_for_media: Option<bool>,
    pub clipboard_monitor: Option<bool>,
    pub proxy_url: Option<String>,
    pub schedule_enabled: Option<bool>,
    pub schedule_start: Option<String>,
    pub schedule_end: Option<String>,
    // ---- IDM-class settings ----
    pub on_complete: Option<String>,
    pub max_retries: Option<i32>,
    pub retry_delay_ms: Option<i64>,
    pub conn_timeout_ms: Option<i64>,
    pub read_timeout_ms: Option<i64>,
    pub exclude_extensions: Option<String>,
    pub exclude_domains: Option<String>,
    /// Allow-list for browser-capture: comma-separated extensions. When
    /// non-empty, ONLY these extensions are captured (e.g. "zip,mp4,iso").
    /// Empty = no allow-list (exclude_* is the only filter).
    pub allowed_extensions: Option<String>,
    /// Extensions the user marked as "Always download this type". When a
    /// browser download's extension matches, ask-mode is bypassed and the
    /// download is captured automatically. Persisted in AppSettings so it
    /// survives Chrome and extension restarts.
    pub always_allow_extensions: Option<String>,
    pub capture_mode: Option<String>, // "ask" | "auto" | "off"
    pub browser_integration: Option<bool>,
    pub notify_on_complete: Option<bool>,
    pub notify_on_fail: Option<bool>,
    pub auto_categorize: Option<bool>,
    pub auto_organize: Option<bool>,
    pub confirm_exit: Option<bool>,
    pub sound_on_complete: Option<bool>,
    /// Safety opt-in for the destructive system-level completion actions
    /// (shutdown / suspend / hibernate). The engine refuses to run them
    /// unless this is true. Default-false so a fresh install cannot
    /// accidentally power off the user's machine.
    pub dangerous_actions: Option<bool>,
}

/// A download queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadQueue {
    pub name: String,
    pub label: String,
    pub max_simultaneous: i32,
    pub speed_limit_kbps: i64,
    pub enabled: bool,
    pub priority: i32,
    pub schedule_enabled: bool,
    pub schedule_start: String,
    pub schedule_end: String,
    pub on_complete: String,
    pub on_complete_command: String,
    /// Phase 3B: queue chaining. When this queue empties, start the
    /// named queue (if it exists, if it is enabled, and if the user's
    /// completion-action settings permit it). Empty string = no chain.
    /// Stored alongside the existing per-queue `on_complete` action —
    /// the completion path runs the on_complete action first, then
    /// promotes the chain target.
    #[serde(default)]
    pub next_queue: String,
    // Per-queue authentication overlay. Empty/None means inherit global.
    // Sensitive values (passwords) are stored in the OS keyring when
    // available; the boolean fields here are non-secret metadata.
    #[serde(default)]
    pub proxy_url: String,
    #[serde(default)]
    pub cookies: String,
    #[serde(default)]
    pub user_agent: String,
    #[serde(default)]
    pub auth_ref: String,
    #[serde(default)]
    pub auth_user: String,
    /// True if a credential has been configured via the keyring for this
    /// queue (the value is not stored in SQLite, only a flag).
    #[serde(default)]
    pub has_password: bool,
}

impl Default for DownloadQueue {
    fn default() -> Self {
        DownloadQueue {
            name: "general".into(),
            label: "General".into(),
            max_simultaneous: 3,
            speed_limit_kbps: 0,
            enabled: true,
            priority: 0,
            schedule_enabled: false,
            schedule_start: "00:00".into(),
            schedule_end: "23:59".into(),
            on_complete: "none".into(),
            on_complete_command: String::new(),
            next_queue: String::new(),
            proxy_url: String::new(),
            cookies: String::new(),
            user_agent: String::new(),
            auth_ref: String::new(),
            auth_user: String::new(),
            has_password: false,
        }
    }
}

/// A file category with default routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub label: String,
    pub extensions: String, // comma-separated, e.g. ".mp4,.mkv"
    pub save_dir: String,
    pub queue: String,
    pub color: Option<String>,
    pub builtin: bool,
}

/// A per-site rule. User-defined configuration applied to downloads
/// from a specific host.
///
/// Precedence (most-specific first):
///   1. exact host match (`site.domain == "cdn.example.com"`)
///   2. explicit subdomain pattern (`site.domain == "example.com"`)
///   3. any rule with empty domain (the "global defaults" rule) — only
///      one is allowed, the engine uses it as a fallback when no host
///      rule matches.
///
/// Rules are not used to bypass authentication controls or site
/// access restrictions. They are configuration only.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SiteRule {
    pub domain: String,
    pub queue: String,
    pub save_dir: String,
    pub auth_ref: String,
    pub proxy_url: String,
    pub auto_capture: bool,
    pub max_connections: i32,
    pub bandwidth_kbps: i64,
    /// Optional category override applied at insert time. When empty,
    /// the engine uses the existing category-resolution logic (extension
    /// match → default category).
    pub category: String,
    /// Optional per-site User-Agent override. When empty, the engine
    /// uses whatever the caller supplied (or the downloader default).
    pub user_agent: String,
    /// Human-readable note shown in the Site Rules UI.
    pub notes: String,
}

/// A scheduled profile (time-window based).
///
/// A schedule can do two things during its active window:
///   1. Apply per-queue bandwidth overrides (a JSON map of queue-name
///      to kbps; value 0 means "unlimited for this queue").
///   2. Optionally start/stop a single legacy queue (the `queue` and
///      `action` fields). New code should prefer `overrides_json`.
///
/// Multiple enabled schedules that overlap are resolved deterministically
/// by `priority` (higher wins), then by `id` (older first).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: i64,
    pub name: String,
    pub queue: String,            // legacy single-queue target
    pub enabled: bool,
    pub kind: String,             // "daily" | "once" | "weekly"
    pub start_time: String,
    pub end_time: String,
    pub days: i32,                // bitmask Sun..Sat; 127 = every day
    pub action: String,           // "start" | "stop" | "set_bw" | "bandwidth"
    pub bandwidth_kbps: i64,      // legacy single-queue kbps
    /// JSON-encoded map of queue_name -> kbps. Missing keys = no override.
    /// When a schedule activates and contains overrides, the engine
    /// remembers each affected queue's previous speed and restores it
    /// when the schedule exits.
    #[serde(default)]
    pub overrides_json: String,
    /// Higher priority wins when multiple enabled schedules overlap.
    /// Ties resolve by `id` (older first) for determinism.
    #[serde(default)]
    pub priority: i32,
    pub on_complete: String,
    pub on_complete_command: String,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            id: 0,
            name: String::new(),
            queue: "general".into(),
            enabled: true,
            kind: "daily".into(),
            start_time: "00:00".into(),
            end_time: "23:59".into(),
            days: 127,
            action: "set_bw".into(),
            bandwidth_kbps: 0,
            overrides_json: "{}".into(),
            priority: 0,
            on_complete: "none".into(),
            on_complete_command: String::new(),
        }
    }
}

/// Built-in download categories (seeded on first run).
pub const BUILTIN_CATEGORIES: &[(&str, &str, &str, &str)] = &[
    ("video", "Video", ".mp4,.mkv,.webm,.avi,.mov,.wmv,.flv,.m4v,.mpg,.ts", "#f87171"),
    ("music", "Music", ".mp3,.flac,.wav,.ogg,.m4a,.aac,.opus", "#34d399"),
    ("compressed", "Compressed", ".zip,.rar,.7z,.tar,.gz,.xz,.bz2,.zst,.iso", "#fbbf24"),
    ("documents", "Documents", ".pdf,.doc,.docx,.xls,.xlsx,.ppt,.pptx,.txt,.md,.odt,.epub,.csv", "#38bdf8"),
    ("programs", "Programs", ".exe,.msi,.deb,.rpm,.AppImage,.flatpak,.dmg,.apk,.run,.sh", "#a855f7"),
    ("images", "Images", ".png,.jpg,.jpeg,.gif,.webp,.bmp,.svg,.heic,.ico", "#22d3ee"),
    ("other", "Other", "", "#64748b"),
];

/// One file link found by the site grabber.
///
/// Cheaply extracted from the page HTML (URL + extension + filename
/// hint). Optional `content_type` / `size` are populated by a separate
/// HEAD probe, batched 8-at-a-time so a 200-link page doesn't
/// hammer the server. `category` is the engine's auto-inferred
/// category from the extension; the UI may override it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrabbedLink {
    pub url: String,
    pub filename: String,
    pub ext: String,
    /// Content-Type from the link's `<meta>` or the response HEAD.
    /// `None` when we never probed (the page is large, the user
    /// asked for the cheap path).
    pub content_type: Option<String>,
    /// Total size in bytes from a HEAD probe. `0` when not probed.
    pub size: i64,
    /// Inferred category (matches the engine's category-resolution
    /// table). `None` when the extension is unknown.
    pub category: Option<String>,
    /// Reason this link passed the include/exclude filter
    /// ("include", "ext", "mime", "size", "default", "excluded:<rule>").
    /// Empty when no rules were applied.
    pub matched: String,
    /// Phase 3C: which HTML attribute the link was extracted from.
    /// `href` for `<a>` / `<link>`, `src` for `<img>` / `<script>`
    /// / `<video>` / `<source>` / `<iframe>`, `meta` for
    /// `<meta http-equiv="refresh">` and similar. The UI shows a
    /// small chip next to the filename so the user can tell where
    /// the link came from.
    #[serde(default)]
    pub source: String,
}

/// User-defined filter for `Download Grabber` results.
///
/// The defaults are IDM-style: a small include list of common
/// downloadable file types, an exclude list of HTML asset types.
/// Setting any of the include_* lists to non-empty turns that list
/// into an ALLOW-list (only matching links are kept).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrabRules {
    /// Allow these extensions (e.g. `["mp4", "mkv", "zip"]`).
    /// When non-empty, links outside this list are dropped.
    pub include_extensions: Vec<String>,
    /// Drop these extensions even if they match include.
    pub exclude_extensions: Vec<String>,
    /// Allow these MIME prefixes (`"video/"`, `"image/"`, etc.).
    /// When non-empty, only matching types are kept.
    pub include_mime: Vec<String>,
    /// Drop these MIME prefixes.
    pub exclude_mime: Vec<String>,
    /// Drop links from these domains (hostname-aware, same
    /// matching rules as Site Rules).
    pub exclude_domains: Vec<String>,
    /// Drop links smaller than this many bytes. `0` = no minimum.
    pub min_size: i64,
    /// Drop links larger than this many bytes. `0` = no maximum.
    pub max_size: i64,
}

impl GrabRules {
    /// Sensible defaults. IDs always pass; obvious HTML assets
    /// (CSS/JS/SVG/PNG/JPG) are excluded because users almost
    /// never want to grab them.
    pub fn sensible_defaults() -> Self {
        Self {
            include_extensions: vec![
                "zip".into(), "7z".into(), "rar".into(),
                "tar".into(), "gz".into(), "bz2".into(), "xz".into(),
                "iso".into(),
                "mp4".into(), "mkv".into(), "webm".into(), "avi".into(), "mov".into(),
                "mp3".into(), "flac".into(), "wav".into(), "m4a".into(), "ogg".into(),
                "pdf".into(), "epub".into(), "doc".into(), "docx".into(), "txt".into(),
                "exe".into(), "msi".into(), "apk".into(), "deb".into(), "rpm".into(),
                "appimage".into(), "dmg".into(), "sh".into(),
            ],
            exclude_extensions: vec![
                "css".into(), "js".into(), "mjs".into(),
                "png".into(), "jpg".into(), "jpeg".into(), "gif".into(),
                "webp".into(), "svg".into(), "ico".into(),
                "woff".into(), "woff2".into(), "ttf".into(), "eot".into(),
                "html".into(), "htm".into(), "json".into(), "xml".into(),
            ],
            include_mime: vec![],
            exclude_mime: vec!["text/css".into(), "application/javascript".into()],
            exclude_domains: vec![],
            min_size: 0,
            max_size: 0,
        }
    }
}

/// Result of probing a URL (HEAD with ranged GET fallback) without
/// inserting anything into the database. Returned to the Add Download
/// dialog so the user can confirm filename / size / type before
/// committing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub url: String,
    pub filename: String,
    pub size: i64,
    pub content_type: Option<String>,
    pub supports_resume: bool,
}


/// A snapshot of which schedules are currently in effect, returned
/// to the UI so it can render an "active profile" indicator without
/// re-implementing the time-window matching logic on the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProfile {
    pub schedule_id: i64,
    pub name: String,
    pub priority: i32,
    /// Queues whose effective speed is currently being overridden
    /// by this profile.
    pub affected_queues: Vec<String>,
}

/// Phase 3B: per-queue live status snapshot returned by
/// `DownloadManager::scheduler_status`. The UI uses this to render
/// the queue details panel without needing to recompute bandwidth,
/// scheduling windows, or the precedence order itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub queue: String,
    pub label: String,
    pub active_downloads: usize,
    pub queued_downloads: usize,
    pub max_simultaneous: i32,
    /// Speed the user configured for this queue (kbps).
    pub configured_speed_kbps: u64,
    /// Speed the engine is currently enforcing (kbps). Equals
    /// `configured_speed_kbps` when no schedule is overriding it.
    pub current_speed_kbps: u64,
    /// If a schedule is currently overriding this queue, this is
    /// the schedule's id; otherwise `None`.
    pub active_schedule_id: Option<i64>,
    /// Whether the queue is currently inside its own
    /// `schedule_enabled` window. Distinct from a bandwidth
    /// profile override — a queue can have a schedule that just
    /// gates start/stop without touching bandwidth.
    pub in_own_schedule_window: bool,
    pub enabled: bool,
    pub priority: i32,
    /// Whether at least one queued task has a non-empty auth_ref
    /// (i.e. the queue is configured for credentials). The UI
    /// shows a small badge next to the queue name.
    pub auth_configured: bool,
}

/// Phase 3B: live snapshot of the scheduler returned by
/// `DownloadManager::scheduler_status`. Designed for the status bar
/// and the queue details panel: a single struct the UI can render
/// without further server round-trips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub active_profiles: Vec<ActiveProfile>,
    pub queues: Vec<QueueStatus>,
    pub global_max_simultaneous: u32,
    pub global_active: usize,
    pub global_queued: usize,
    pub time_now: String,
}
/// Parsed chunk-range response header helper.
pub fn parse_content_range_total(value: &str) -> Option<i64> {
    let total = value.rsplit('/').next()?;
    if total == "*" {
        return None;
    }
    total.parse().ok()
}

/// Result of a one-shot checksum verification.
///
/// `algorithm` is always `"sha256"` for now; the field exists so the
/// frontend can render the algorithm name without hardcoding it.
///
/// `computed` is the lowercase hex digest of the file as it sits on
/// disk. `expected` is whatever the user entered (the `expected_sha256`
/// column); empty string means "no expected value, just recorded the
/// actual digest".
///
/// `matches` is `Some(true)` if the computed digest matched the expected
/// one (case-insensitive), `Some(false)` if it didn't, or `None` if
/// there was no expected digest to compare against.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChecksumResult {
    pub algorithm: String,
    pub computed: String,
    pub expected: String,
    pub matches: Option<bool>,
    pub file_size: u64,
}
