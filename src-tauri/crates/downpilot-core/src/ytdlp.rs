//! Wrapper around yt-dlp + ffmpeg for media probing and downloading.
//!
//! Probing runs `yt-dlp -J` and curates quick-pick formats (best / 1080p /
//! 720p / audio-only). Media downloads run yt-dlp directly so HLS/DASH
//! manifests and video+audio muxing (via ffmpeg) are handled correctly;
//! progress is parsed from yt-dlp's newline output.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::models::{EngineEvent, MediaFormat, MediaInfo, ProbeOptions};

#[derive(Debug, thiserror::Error)]
pub enum YtDlpError {
    #[error("yt-dlp binary not found on PATH; install it (pipx install yt-dlp or flatpak)")]
    BinaryNotFound,
    #[error("yt-dlp exited with code {code:?}: {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("invalid yt-dlp JSON output: {0}")]
    BadJson(String),
    #[error("cancelled")]
    Cancelled,
    #[error("timed out: {0}")]
    Timeout(String),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct YtDlp;

/// Cookies from the browser extension arrive as a raw `document.cookie`
/// header string ("a=1; b=2"). yt-dlp deprecates
/// `--add-header "Cookie: ..."` (and scopes it oddly), so we convert it to a
/// Netscape cookie jar and pass `--cookies <file>` instead.
static COOKIE_SEQ: AtomicU64 = AtomicU64::new(0);

fn write_cookie_jar(cookies: &str, url: &str) -> Option<PathBuf> {
    let cookies = cookies.trim();
    if cookies.is_empty() {
        return None;
    }
    let host = url
        .splitn(3, '/')
        .nth(2)
        .and_then(|s| s.split('/').next())
        .unwrap_or("")
        .trim_start_matches('.');
    if host.is_empty() {
        return None;
    }
    let mut body = String::from("# Netscape HTTP Cookie File\n");
    for pair in cookies.split(';') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (name, value) = match pair.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (pair, ""),
        };
        // Netscape cookie file format:
        //   domain \t domain_specified \t path \t secure \t expiry \t name \t value
        // `domain_specified` MUST equal whether `domain` begins with a dot:
        //   host-only  -> "www.xx" + FALSE
        //   domain-wide -> ".xx"     + TRUE
        // (Python's http.cookiejar asserts this; a mismatch breaks yt-dlp.)
        let domains = vec![
            (host.to_string(), "FALSE"),
            (format!(".{host}"), "TRUE"),
        ];
        for (d, flag) in domains {
            body.push_str(&format!(
                "{}\t{}\t/\tFALSE\t2147483647\t{}\t{}\n",
                d, flag, name, value
            ));
        }
    }
    let seq = COOKIE_SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir()
        .join(format!("downpilot-cookies-{}-{}.txt", std::process::id(), seq));
    std::fs::write(&path, body).ok()?;
    Some(path)
}

/// Deletes any temp cookie jars it holds when dropped (covers early returns).
struct CookieFiles(Vec<PathBuf>);
impl Drop for CookieFiles {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

impl YtDlp {
    pub fn is_available() -> bool {
        std::process::Command::new("yt-dlp")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_child| true)
            .unwrap_or(false)
    }

    /// Probe a URL and return curated media info.
    pub async fn probe(url: &str, opts: &ProbeOptions) -> Result<MediaInfo, YtDlpError> {
        let mut cmd = Command::new("yt-dlp");
        cmd.arg("-J").arg("--no-playlist").arg("--no-warnings").arg(url);
        if let Some(p) = &opts.proxy {
            cmd.arg("--proxy").arg(p);
        }
        if let Some(c) = &opts.cookies {
            if let Some(jar) = write_cookie_jar(c, url) {
                cmd.arg("--cookies").arg(&jar);
            }
        }
        if let Some(ua) = &opts.user_agent {
            cmd.arg("--user-agent").arg(ua);
        }
        if let Some(r) = &opts.referer {
            cmd.arg("--referer").arg(r);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // If the probe future is dropped (e.g. the 60s timeout below fires),
        // kill the child instead of leaking an orphaned yt-dlp process.
        cmd.kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                YtDlpError::BinaryNotFound
            } else {
                YtDlpError::Io(e)
            }
        })?;

        let out = tokio::time::timeout(Duration::from_secs(60), read_output(child)).await
            .map_err(|_| YtDlpError::Failed { code: None, stderr: "probe timed out after 60s".into() })??;

        if !out.status {
            return Err(YtDlpError::Failed { code: out.code, stderr: out.stderr });
        }

        let v: Value = serde_json::from_str(&out.stdout).map_err(|e| YtDlpError::BadJson(e.to_string()))?;
        build_media_info(v, url)
    }

    /// Download media via yt-dlp into `save_dir`. Emits progress events.
    /// Returns the final file path. Cancellation sends SIGTERM (yt-dlp's
    /// .part files resume cleanly on restart with the same output template).
    #[allow(clippy::too_many_arguments)]
    pub async fn download(
        url: &str,
        format_id: &str,
        save_dir: &Path,
        referer: Option<&str>,
        user_agent: Option<&str>,
        cookies: Option<&str>,
        proxy: Option<&str>,
        cancel: Arc<AtomicBool>,
        tx: tokio::sync::mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<PathBuf, YtDlpError> {
        let template = save_dir.join("%(title)s [%(id)s].%(ext)s");
        let mut cmd = Command::new("yt-dlp");
        cmd.arg("--no-playlist")
            .arg("-f").arg(format_id)
            .arg("-o").arg(template.to_string_lossy().to_string())
            .arg("--newline")
            .arg("--no-warnings")
            .arg("--progress")
            .arg("--merge-output-format").arg("mp4")
            .arg(url);
        if let Some(p) = proxy {
            cmd.arg("--proxy").arg(p);
        }
        if let Some(r) = referer {
            cmd.arg("--referer").arg(r);
        }
        if let Some(ua) = user_agent {
            cmd.arg("--user-agent").arg(ua);
        }
        let _cookie_guard;
        if let Some(c) = cookies {
            if let Some(jar) = write_cookie_jar(c, url) {
                cmd.arg("--cookies").arg(&jar);
                _cookie_guard = CookieFiles(vec![jar]);
            } else {
                _cookie_guard = CookieFiles(vec![]);
            }
        } else {
            _cookie_guard = CookieFiles(vec![]);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                YtDlpError::BinaryNotFound
            } else {
                YtDlpError::Io(e)
            }
        })?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let mut err_lines = BufReader::new(stderr).lines();
        let mut err_buf = String::new();

        let progress_re = Regex::new(r"\[download\]\s+([\d.]+)%\s+of\s+~?([\d.]+)(KiB|MiB|GiB)").unwrap();

        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = child.kill().await;
                        let _ = child.wait().await;
                        return Err(YtDlpError::Cancelled);
                    }
                }
                line = lines.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if let Some(caps) = progress_re.captures(&l) {
                                if let (Some(pct), Some(amount)) = (caps.get(1), caps.get(2)) {
                                    let pct: f64 = pct.as_str().parse().unwrap_or(0.0);
                                    let unit = caps.get(3).map(|m| m.as_str()).unwrap_or("MiB");
                                    let mult = match unit { "KiB" => 1024.0, "GiB" => 1024.0*1024.0*1024.0, _ => 1024.0*1024.0 };
                                    let bytes = amount.as_str().parse::<f64>().unwrap_or(0.0) * mult;
                                    let _ = tx.send(EngineEvent::Progress {
                                        id: -1, downloaded: bytes as i64, total: (bytes / pct.max(0.001) * 100.0) as i64, speed_bps: 0.0,
                                    });
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                err_line = err_lines.next_line() => {
                    if let Ok(Some(l)) = err_line { err_buf.push_str(&l); err_buf.push('\n'); }
                }
            }
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(YtDlpError::Failed { code: status.code(), stderr: err_buf });
        }
        // Find the produced file (newest matching entry in save_dir).
        let mut dir = tokio::fs::read_dir(save_dir).await?;
        let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
        while let Ok(Some(entry)) = dir.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if meta.is_file() && !entry.file_name().to_string_lossy().ends_with(".part") {
                    if let Ok(mtime) = meta.modified() {
                        if newest.as_ref().map(|(_, t)| mtime > *t).unwrap_or(true) {
                            newest = Some((entry.path(), mtime));
                        }
                    }
                }
            }
        }
        newest.map(|(p, _)| p).ok_or_else(|| YtDlpError::Failed { code: None, stderr: "no output file found".into() })
    }
}

struct ChildOutput {
    stdout: String,
    stderr: String,
    status: bool,
    code: Option<i32>,
}

async fn read_output(child: tokio::process::Child) -> Result<ChildOutput, YtDlpError> {
    let out = child.wait_with_output().await?;
    Ok(ChildOutput {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        status: out.status.success(),
        code: out.status.code(),
    })
}

fn build_media_info(v: Value, fallback_url: &str) -> Result<MediaInfo, YtDlpError> {
    let title = v["title"].as_str().unwrap_or("untitled").to_string();
    let webpage_url = v["webpage_url"].as_str().unwrap_or(fallback_url).to_string();
    let thumbnail = v["thumbnail"].as_str().map(|s| s.to_string());
    let duration = v["duration"].as_f64();
    let uploader = v["uploader"].as_str().map(|s| s.to_string());

    let mut formats: Vec<MediaFormat> = Vec::new();
    if let Some(arr) = v["formats"].as_array() {
        for f in arr {
            let vcodec = f["vcodec"].as_str().map(|s| s.to_string()).filter(|s| s != "none");
            let acodec = f["acodec"].as_str().map(|s| s.to_string()).filter(|s| s != "none");
            let height = f["height"].as_i64();
            let fps = f["fps"].as_f64();
            let resolution = match (height, f["width"].as_i64()) {
                (Some(h), Some(w)) => Some(format!("{w}x{h}")),
                (Some(h), None) => Some(format!("{h}p")),
                _ => None,
            };
            let filesize = f["filesize"].as_i64().or_else(|| f["filesize_approx"].as_i64());
            formats.push(MediaFormat {
                format_id: f["format_id"].as_str().unwrap_or("?").to_string(),
                ext: f["ext"].as_str().unwrap_or("?").to_string(),
                resolution,
                fps,
                vcodec,
                acodec,
                filesize,
                note: f["format_note"].as_str().map(|s| s.to_string()),
                url: f["url"].as_str().map(|s| s.to_string()),
                protocol: f["protocol"].as_str().map(|s| s.to_string()),
            });
        }
    }

    // Curated quick picks.
    let mut quick_picks: Vec<MediaFormat> = Vec::new();
    let mut best: Option<MediaFormat> = None;
    let mut p1080: Option<MediaFormat> = None;
    let mut p720: Option<MediaFormat> = None;
    let mut audio: Option<(f64, MediaFormat)> = None; // (abr, format)

    for f in &formats {
        let is_video = f.vcodec.is_some() && f.acodec.is_some();
        let h = f.resolution.as_ref().and_then(|r| {
            r.rsplit('x').next().and_then(|s| s.trim_end_matches('p').parse::<i64>().ok())
        });
        if is_video && f.protocol.as_deref() != Some("mhtml") {
            if h == Some(1080) && p1080.is_none() { p1080 = Some(f.clone()); }
            if h == Some(720) && p720.is_none() { p720 = Some(f.clone()); }
            if best.is_none() || h.unwrap_or(0) > best_res(&best) {
                best = Some(f.clone());
            }
        }
        if f.vcodec.is_none() && f.acodec.is_some() {
            let abr = f.note.as_deref().and_then(|n| n.parse::<f64>().ok()).unwrap_or(0.0);
            if audio.as_ref().map(|(a, _)| abr > *a).unwrap_or(true) {
                audio = Some((abr, f.clone()));
            }
        }
    }

    let mk = |label: &str, f: &MediaFormat| MediaFormat {
        note: Some(label.to_string()),
        ..f.clone()
    };
    if let Some(f) = best { quick_picks.push(mk("Best quality", &f)); }
    if let Some(f) = p1080 { quick_picks.push(mk("1080p", &f)); }
    if let Some(f) = p720 { quick_picks.push(mk("720p", &f)); }
    if let Some((_, f)) = audio { quick_picks.push(mk("Audio only", &f)); }
    // Fallback special IDs handled by yt-dlp at download time.
    if quick_picks.is_empty() {
        quick_picks.push(MediaFormat {
            format_id: "bv*+ba/b".into(), ext: "mp4".into(), resolution: None, fps: None,
            vcodec: Some("any".into()), acodec: Some("any".into()), filesize: None,
            note: Some("Best quality".into()), url: None, protocol: None,
        });
        quick_picks.push(MediaFormat {
            format_id: "ba/b".into(), ext: "m4a".into(), resolution: None, fps: None,
            vcodec: None, acodec: Some("any".into()), filesize: None,
            note: Some("Audio only".into()), url: None, protocol: None,
        });
    }

    let ext = quick_picks.first().map(|f| f.ext.as_str()).unwrap_or("mp4");
    let default_filename = format!("{title}.{ext}");

    Ok(MediaInfo {
        title,
        webpage_url,
        thumbnail,
        duration,
        uploader,
        formats,
        quick_picks,
        default_filename,
    })
}

fn best_res(f: &Option<MediaFormat>) -> i64 {
    f.as_ref().and_then(|f| f.resolution.as_ref())
        .and_then(|r| r.rsplit('x').next().and_then(|s| s.trim_end_matches('p').parse().ok()))
        .unwrap_or(0)
}
