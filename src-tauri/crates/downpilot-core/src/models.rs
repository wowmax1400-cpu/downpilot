use serde::{Deserialize, Serialize};

/// Lifecycle state of a download task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Cancelled => "cancelled",
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
    pub speed_bps: f64,
    pub threads: u32,
    pub format_id: Option<String>,
    pub webpage_url: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: Option<String>,
    pub category: String,
    pub expected_sha256: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DownloadTask {
    pub fn progress_ratio(&self) -> f64 {
        if self.total_size > 0 {
            (self.downloaded as f64 / self.total_size as f64).clamp(0.0, 1.0)
        } else {
            0.0
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
    pub expected_sha256: Option<String>,
    #[serde(default = "default_true")]
    pub start_immediately: bool,
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
    /// Curated quick picks: best, 1080p, 720p, audio-only.
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
    pub speed_limit_kbps: Option<u64>,
    pub theme: Option<String>,
    pub accent: Option<String>,
    pub auto_intercept: Option<bool>,
    pub use_ytdlp_for_media: Option<bool>,
    /// HTTP/SOCKS proxy for all downloads (e.g. http://127.0.0.1:2080,
    /// socks5://127.0.0.1:1080). Empty = system default (no proxy).
    pub proxy_url: Option<String>,
    /// Schedule downloads to a daily window: "HH:MM" 24h format.
    pub schedule_enabled: Option<bool>,
    pub schedule_start: Option<String>,
    pub schedule_end: Option<String>,
}

/// One file link found by the site grabber.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrabbedLink {
    pub url: String,
    pub filename: String,
    pub ext: String,
}

/// Parsed chunk-range response header helper.
pub fn parse_content_range_total(value: &str) -> Option<i64> {
    // "bytes 0-1/12345" -> 12345
    let total = value.rsplit('/').next()?;
    if total == "*" {
        return None;
    }
    total.parse().ok()
}
